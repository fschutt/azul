---
slug: internals/event-system
title: Event System Internals
language: en
canonical_slug: internals/event-system
audience: contributor
maturity: mature
guide_order: null
topic_only: false
short_desc: Hit-testing, callback invocation, the Update protocol
prerequisites: []
tracked_files:
  - core/src/callbacks.rs
  - core/src/events.rs
  - core/src/hit_test.rs
  - core/src/hit_test_tag.rs
  - core/src/drag.rs
  - core/src/selection.rs
  - layout/src/default_actions.rs
  - layout/src/hit_test.rs
  - layout/src/managers/virtual_view.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:52Z
---

# Event System Internals

Every interactive callback in azul reaches the user the same way: the platform shell pushes a raw OS event into `FullWindowState`, the input interpreter turns it into one or more `SyntheticEvent`s plus framework-internal `SystemChange`s, the dispatcher walks the DOM in capture/target/bubble order to collect matching `EventFilter`s, the user callbacks run against the matched nodes, and the unprevented default actions are applied last. The pipeline lives in [`core/src/events.rs`](../../../../core/src/events.rs), [`layout/src/default_actions.rs`](../../../../layout/src/default_actions.rs), and the dispatch glue in [`dll/src/desktop/shell2/common/event.rs`](../../../../dll/src/desktop/shell2/common/event.rs).

```text
OS event ─► FullWindowState diff ─► SyntheticEvent
                  │
                  ▼
   default_input_interpreter()       (events.rs:2773)
   ├─► SystemChange                  (apply_system_change → managers)
   └─► user_events
                  │
                  ▼
   dispatch_events_propagated()      (event.rs)
   ├─► event_type_to_filters()       (events.rs:2206)
   └─► propagate_event()             (events.rs:793)
            ├─► Capture
            ├─► Target
            └─► Bubble
                  │
                  ▼
   Callback returns Update
                  │
                  ▼
   determine_keyboard_default_action()   (default_actions.rs:49)
   default_post_filter()                 (events.rs:3056)
```

## Pipeline order in `process_events`

The shell entry point `PlatformWindowV2::process_events` (in [`dll/src/desktop/shell2/common/event.rs`](../../../../dll/src/desktop/shell2/common/event.rs)) executes the steps below for every input batch:

1. **State diff.** The shell mutates `current_window_state` with raw input. Diffing it against `previous_window_state` produces `SyntheticEvent`s for cursor moves, button transitions, key presses, focus, theme changes, etc.
2. **Manager events.** Managers that need temporal context (`GestureManager`, `ScrollManager`, `CursorManager`, `TextEditManager`) implement `EventProvider::get_pending_events` and contribute additional `SyntheticEvent`s.
3. **Pre-callback filter.** `default_input_interpreter` (overridable via `InputInterpreterCallback` on `LayoutWindow`) folds those events into a `PreCallbackFilterResult { system_changes, user_events }`. System changes are applied immediately (focus, scroll, drag activation, selection updates).
4. **Dispatch.** `dispatch_events_propagated(&user_events)` runs each event through `propagate_event` and invokes the planned `CoreCallback`s. Callbacks return `Update` and may call `event.prevent_default()`, `stop_propagation()`, or `stop_immediate_propagation()`.
5. **Default actions.** If no callback prevented default and any `KeyDown` was in the batch, `determine_keyboard_default_action` ([`layout/src/default_actions.rs:49`](../../../../layout/src/default_actions.rs)) returns a `DefaultActionResult`. Tab/Shift+Tab/Home+Ctrl/End+Ctrl/Escape are converted via `default_action_to_focus_target` and applied through `SystemChange::SetFocus`. Enter/Space on activatable elements synthesise a `Click` event and re-enter dispatch.
6. **Post filter.** `default_post_filter` (overridable via `PostFilterCallback`) inspects `(prevent_default, pre_changes, old_focus, new_focus)` and emits final `SystemChange`s — clearing selections on focus change, finalising IME composition state, scrolling the new focus into view.

The dispatch loop recurses up to a fixed `MAX_EVENT_RECURSION_DEPTH` so that `Update::RefreshDom` returned from a callback rebuilds the DOM, runs lifecycle reconciliation, and re-enters event delivery for synthetic Mount/Unmount/Resize events.

## `SyntheticEvent`

```rust,ignore
pub struct SyntheticEvent {
    pub event_type: EventType,
    pub source: EventSource,        // User | Programmatic | Synthetic | Lifecycle
    pub phase: EventPhase,          // Capture | Target | Bubble
    pub target: DomNodeId,
    pub current_target: DomNodeId,  // updated as propagation walks the path
    pub timestamp: Instant,
    pub data: EventData,            // Mouse | Keyboard | Scroll | Touch | Clipboard | Lifecycle | Window | None
    pub stopped: bool,
    pub stopped_immediate: bool,
    pub prevented_default: bool,
}
```

Defined at [`core/src/events.rs:646`](../../../../core/src/events.rs). The `source` field is load-bearing: `EventSource::Lifecycle` short-circuits propagation (`propagate_target_phase` is the only phase used), `EventSource::Synthetic` events generated by the framework (e.g. activation clicks) re-enter dispatch as if they were user events, and `EventSource::Programmatic` is set on API-driven scrolls and focus changes so that scroll callbacks can distinguish.

`stop_propagation()` halts the current phase boundary (capture stops before target, target stops before bubble). `stop_immediate_propagation()` additionally drops remaining handlers on the current node. `prevent_default()` only suppresses the post-dispatch default action — it does not stop callback delivery.

## `EventType` and `EventData`

`EventType` ([`core/src/events.rs:463`](../../../../core/src/events.rs)) is the W3C-aligned superset: mouse, keyboard, IME composition, focus, input/change/submit, scroll, drag, touch, gesture, clipboard, media, lifecycle, window, application, file. `EventData` carries type-specific payloads (`MouseEventData`, `KeyboardEventData`, `ScrollEventData`, `TouchEventData`, `ClipboardEventData`, `LifecycleEventData`, `WindowEventData`, or `None`). The data variant must match the event type — `dispatch_events_propagated` does not validate this; callers (the input interpreter, manager `get_pending_events` impls) are responsible.

`KeyModifiers { shift, ctrl, alt, meta }` is duplicated inside `MouseEventData` and `KeyboardEventData` rather than read from `KeyboardState` because gesture managers may produce events with stale modifier snapshots.

## `EventFilter` taxonomy

Callbacks are registered against one of five filter categories ([`core/src/events.rs:2040`](../../../../core/src/events.rs)):

| Filter | Fires on | Notes |
|---|---|---|
| `Hover(HoverEventFilter)` | Nodes hit by the cursor | W3C capture/target/bubble through the DOM path |
| `Focus(FocusEventFilter)` | The focused node only | No propagation; node must be in the tab order |
| `Window(WindowEventFilter)` | Every node with a matching callback | Brute-force fan-out across all DOMs |
| `Component(ComponentEventFilter)` | The reconciler's target node | `AfterMount`, `BeforeUnmount`, `Updated`, `NodeResized` |
| `Application(ApplicationEventFilter)` | Same as Window | Reserved for monitor-connect/disconnect-style events |

`EventFilter::Not` exists in the type but `matches_filter_phase` returns `false` for it — registering a `Not` filter today never fires.

`From<On> for EventFilter` routes the public `On` enum to the right category. Two cases are non-obvious:

- `On::TextInput` becomes `Focus(TextInput)` — text input is delivered to whatever currently owns focus, not to whichever node was hit.
- `On::VirtualKeyDown` / `On::VirtualKeyUp` become `Window(VirtualKeyDown/Up)` — keyboard events fan out window-wide so layout-driven shortcuts can register on the root.

## `event_type_to_filters`

```rust,ignore
pub fn event_type_to_filters(event_type: EventType, event_data: &EventData) -> Vec<EventFilter>;
```

Defined at [`core/src/events.rs:2206`](../../../../core/src/events.rs). One incoming event can match several filters — `EventType::MouseDown` with a `MouseEventData { button: Left, .. }` returns both the generic `Hover(MouseDown)` and the button-specific `Hover(LeftMouseDown)`. `EventType::Click` only matches `Hover(LeftMouseDown)` (W3C: click is left-button only). Drag events fan out to both `Hover(...)` and `Window(...)` so a global drop handler on the root works.

This function is the single source of truth for the dispatch plan. `propagate_event` uses it implicitly by reading the per-node filter list.

## Capture/target/bubble in `propagate_event`

```rust,ignore
pub fn propagate_event(
    event: &mut SyntheticEvent,
    node_hierarchy: &NodeHierarchy,
    callbacks: &BTreeMap<NodeId, Vec<EventFilter>>,
) -> PropagationResult;
```

[`core/src/events.rs:793`](../../../../core/src/events.rs). The path is computed by walking `parent` pointers from the target to the root, then reversed:

1. **Capture phase**: ancestors in root → target order (excluding the target itself), with `event.phase = EventPhase::Capture`.
2. **Target phase**: the target node alone, with `event.phase = EventPhase::Target`.
3. **Bubble phase**: ancestors in target → root order, with `event.phase = EventPhase::Bubble`.

Each phase iterates nodes; if `event.stopped_immediate` is set the loop returns immediately, if `event.stopped` is set the next phase is skipped. `current_target` is rewritten to the visiting node before each filter check.

Today `matches_filter_phase` does not consult `current_phase`, so a registered `Hover(MouseDown)` callback fires once per phase the node appears in. The phase enum is exposed for future per-phase registration; do not assume otherwise when reading callback counts.

`PropagationResult { callbacks_to_invoke: Vec<(NodeId, EventFilter)>, default_prevented }` is the dispatch plan returned to the shell. `default_prevented` is the OR of every `event.prevented_default` flip during propagation.

## Filter matchers

`matches_filter_phase` dispatches to four predicates:

- `matches_hover_filter` ([`core/src/events.rs:1122`](../../../../core/src/events.rs))
- `matches_focus_filter` ([`core/src/events.rs:1171`](../../../../core/src/events.rs))
- `matches_window_filter` ([`core/src/events.rs:1214`](../../../../core/src/events.rs))
- `matches_component_filter` ([`core/src/events.rs:1098`](../../../../core/src/events.rs))

Each is a long match table mapping filter variant × `EventType` to a boolean. Mouse-button filters (`LeftMouseDown` etc.) call `check_mouse_button(&event.data, expected)` to confirm `EventData::Mouse(_).button` matches. Gesture variants (`LongPress`, `SwipeLeft`, `PinchIn`, `RotateClockwise`, …) and the IME composition variants are present in the enums but the matchers currently fall through `_ => false` for them in the hover/focus/window paths — `GestureManager` handles those events through its own dispatch, not through `propagate_event`.

## Default actions

```rust,ignore
pub enum DefaultAction {
    FocusNext, FocusPrevious, FocusFirst, FocusLast, ClearFocus,
    ActivateFocusedElement { target: DomNodeId },
    SubmitForm  { form_node:  DomNodeId },
    CloseModal  { modal_node: DomNodeId },
    ScrollFocusedContainer { direction: ScrollDirection, amount: ScrollAmount },
    SelectAllText,
    None,
}
```

Defined at [`core/src/events.rs:924`](../../../../core/src/events.rs). The function that produces them is in the layout crate (it needs the styled DOM to query `is_activatable` / `is_text_input`):

```rust,ignore
pub fn determine_keyboard_default_action(
    keyboard_state: &KeyboardState,
    focused_node: Option<DomNodeId>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    prevented: bool,
) -> DefaultActionResult;
```

[`layout/src/default_actions.rs:49`](../../../../layout/src/default_actions.rs). The mapping it currently implements:

| Key | Modifiers | Default action |
|---|---|---|
| `Tab` | — | `FocusNext` |
| `Tab` | `Shift` | `FocusPrevious` |
| `Tab` | `Ctrl` / `Alt` | `None` (OS handles it) |
| `Enter` / `NumpadEnter` | — | `ActivateFocusedElement` if `is_activatable` |
| `Space` | — | `ActivateFocusedElement` if activatable and not a text input |
| `Escape` | — | `ClearFocus` if a node is focused |
| `Up` / `Down` / `Left` / `Right` | — | `ScrollFocusedContainer { Line }` outside text inputs |
| `PageUp` / `PageDown` | — | `ScrollFocusedContainer { Page }` |
| `Home` / `End` | — | `ScrollFocusedContainer { Document }` |
| `Home` / `End` | `Ctrl` | `FocusFirst` / `FocusLast` |

`SubmitForm`, `CloseModal`, and `SelectAllText` exist in the enum but no key combination produces them yet — the shell handler matches them and falls through to a `// Placeholder for future implementation` comment.

`default_action_to_focus_target` ([`layout/src/default_actions.rs:238`](../../../../layout/src/default_actions.rs)) bridges the focus variants to the `FocusTarget` consumed by `FocusManager::resolve_focus_target`. `create_activation_click_event` ([`layout/src/default_actions.rs:254`](../../../../layout/src/default_actions.rs)) builds the `SyntheticEvent { event_type: Click, source: Synthetic, … }` re-fed to `dispatch_events_propagated` for Enter/Space activation.

## Pre-callback interpreter (`SystemChange`)

`SystemChange` ([`core/src/events.rs:2348`](../../../../core/src/events.rs)) is the framework-side counterpart to user callbacks. Variants cover text selection, IME, drag-and-drop, focus, scroll, and the auto-scroll timer. They are produced by `default_input_interpreter` and consumed by `apply_system_change` on the shell — adding a variant deliberately causes a compile error there, so every change is handled exhaustively.

```rust,ignore
pub struct InputInterpreterInfo<'a> {
    pub events: &'a [SyntheticEvent],
    pub hit_test: Option<&'a FullHitTest>,
    pub keyboard_state: &'a KeyboardState,
    pub mouse_state: &'a MouseState,
    pub state: InputInterpreterState,  // focused_node, click_count, drag_start_position, has_selection
}

pub type InputInterpreterCallbackType = extern "C" fn(
    RefAny,
    *const InputInterpreterInfo<'static>,
) -> PreCallbackFilterResult;
```

Replace `LayoutWindow::input_interpreter_callback` to implement vim modes, game controls, or custom shortcut tables. Native Rust callers wrap a `fn` via `InputInterpreterCallback::from(fn_ptr)` (sets `ctx = None`); FFI callers use the trampoline pattern with `ctx: OptionRefAny` holding the foreign callable.

Three helper enums live alongside:

- `ArrowDirection::from_key(vk, ctrl)` maps `(VirtualKeyCode, ctrl)` to `Left/Right/Up/Down/LineStart/LineEnd/DocumentStart/DocumentEnd`.
- `KeyboardShortcut::from_key(vk, ctrl, shift)` recognises `Ctrl+C/X/V/A/Z` and `Ctrl+Y` / `Ctrl+Shift+Z`.
- `SelectionOp { direction, step, mode, repeat }` is the unified cursor/selection/delete operation produced by the interpreter from arrow/backspace/delete keys.

## Post-callback filter

```rust,ignore
pub type PostFilterCallbackType = extern "C" fn(
    RefAny,
    bool,                    // prevent_default
    SystemChangeVecSlice,    // pre_changes
    DomNodeId,               // old_focus (0xFFFF = None)
    DomNodeId,               // new_focus
) -> SystemChangeVec;
```

[`core/src/events.rs:2540`](../../../../core/src/events.rs). Runs after user callbacks return, given the merged `prevent_default` flag, the `SystemChange`s the interpreter produced before dispatch, and the focus delta. It returns more `SystemChange`s — typically `ClearAllSelections`, `FinalizePendingFocusChanges`, `ScrollSelectionIntoView`. The default impl is `default_post_filter` ([`core/src/events.rs:3056`](../../../../core/src/events.rs)); override `LayoutWindow::post_filter_callback` to customise.

## Lifecycle reconciliation

```rust,ignore
pub fn detect_lifecycle_events_with_reconciliation(
    dom_id: DomId,
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_hierarchy: &[NodeHierarchyItem],
    new_hierarchy: &[NodeHierarchyItem],
    old_layout: &OrderedMap<NodeId, LogicalRect>,
    new_layout: &OrderedMap<NodeId, LogicalRect>,
    timestamp: Instant,
) -> LifecycleEventResult;
```

[`core/src/events.rs:1482`](../../../../core/src/events.rs). After a `RefreshDom` rebuild the reconciler emits `Mount`, `Unmount`, `Resize`, `Update` synthetic events tagged `EventSource::Lifecycle`. It also returns `node_id_mapping: OrderedMap<old NodeId, new NodeId>` so the shell can migrate focus, scroll position, drag context and selection across the rebuild. The match strategy is: stable reconciliation key first (`.with_reconciliation_key()`), then content hash, then mount/unmount fallback. The simpler index-based `detect_lifecycle_events` ([`core/src/events.rs:1276`](../../../../core/src/events.rs)) exists for cases where reconciliation isn't required.

`Component` filters fire only on the lifecycle event's `target` — `propagate_event` is bypassed for them and `matches_component_filter` is the predicate the dispatcher consults.

## Callback invocation surface

User callbacks attach to `NodeData` as `CoreCallbackData { event: EventFilter, callback: CoreCallback, refany: RefAny }`. `CoreCallback` stores the function pointer as a `usize` plus an optional FFI `ctx: OptionRefAny`:

```rust,ignore
pub type CoreCallbackType = usize;  // actually: extern "C" fn(RefAny, CallbackInfo) -> Update

pub struct CoreCallback {
    pub cb: CoreCallbackType,
    pub ctx: OptionRefAny,
}

pub struct CoreCallbackData {
    pub event: EventFilter,
    pub callback: CoreCallback,
    pub refany: RefAny,
}
```

The `usize` masks a circular dependency: the real callback signature is in `azul-layout` (`CallbackType` and the `CallbackInfo` struct), but `azul-core` has to store the pointer without depending on layout. The dispatcher in the shell is the only code that performs the unsafe `transmute` back to the function pointer at invoke time; everything in `azul-core` keeps it opaque.

The same pattern recurs for image rendering (`CoreRenderImageCallbackType`), input interpreter (`InputInterpreterCallbackType`), and post-filter (`PostFilterCallbackType`). The "Core" prefix marks the usize-stored variant; the layout-side type without the prefix is the real signature.

`Update`, the callback return value, has three levels:

```rust,ignore
pub enum Update {
    DoNothing,
    RefreshDom,             // rebuild the DOM for this window
    RefreshDomAllWindows,
}
```

`Update::max_self` ([`core/src/callbacks.rs`](../../../../core/src/callbacks.rs)) merges results across all callbacks in a batch; the dispatcher uses the merged value to decide whether to recurse with a fresh layout pass.

## Event source distinctions

The shell sets `EventSource` deliberately so downstream consumers can diverge:

- `EventSource::User` — direct OS input (mouse, keyboard, touch).
- `EventSource::Programmatic` — produced by `CallbackInfo::scroll_node_into_view`, `set_focus`, etc. Scroll callbacks should treat this as authoritative and not retrigger.
- `EventSource::Synthetic` — emitted by the framework on behalf of the user. The clearest example is `create_activation_click_event` for Enter/Space activation. Treated identically to `User` by callback filters.
- `EventSource::Lifecycle` — emitted by `detect_lifecycle_events*` after a DOM rebuild. Bypasses `propagate_event` (target-only).

## Callback invocation paths

The shell drives four callback paths through `LayoutWindow`, all wrapping the same six-step pattern (build `CallbackInfo`, invoke the callback, drain the `Arc<Mutex<Vec<CallbackChange>>>`, apply changes via `apply_callback_changes`, merge into `CallCallbacksResult`, return):

| Path | Trigger | Driven by |
|---|---|---|
| `run_single_timer` | A `Timer` expired | `invoke_expired_timers` in the shell tick |
| `run_all_threads` | Background `Thread` posted a message | `invoke_thread_callbacks` after epoll/select |
| `invoke_single_callback` | One filter matched during dispatch | `dispatch_events_propagated` |
| `invoke_menu_callback` | Native menu item clicked | platform menu handlers (macOS/Win/Linux) |

The unification proposal in `scripts/CALLBACK_INVOCATION_UNIFICATION.md` collapses the duplication into a `CallbackChangeResult::merge_into` method plus a generic `invoke_and_collect` helper. The audit at the bottom of that doc lists six fields (`image_callbacks_changed`, `update_all_image_callbacks`, `queued_window_states`, `text_input_triggered`, …) that today are forwarded only on the timer path; treat that as the canonical to-do list when adding new fields to `CallbackChangeResult`.

## Where the pieces live

| Concern | File |
|---|---|
| `SyntheticEvent`, `EventType`, `EventData`, `EventFilter`, `propagate_event`, default-action enums | [`core/src/events.rs`](../../../../core/src/events.rs) |
| `default_input_interpreter`, `SystemChange`, `SelectionOp`, `KeyboardShortcut`, `ArrowDirection`, `default_post_filter` | [`core/src/events.rs`](../../../../core/src/events.rs) (lower half, ~line 2348 onwards) |
| `Update`, `CoreCallback`, `CoreCallbackData`, `LayoutCallback`, FFI trampoline pattern | [`core/src/callbacks.rs`](../../../../core/src/callbacks.rs) |
| `determine_keyboard_default_action`, `default_action_to_focus_target`, `create_activation_click_event` | [`layout/src/default_actions.rs`](../../../../layout/src/default_actions.rs) |
| Dispatch loop (`dispatch_events_propagated`), recursion guard, default-action wiring, synthetic-click re-entry | [`dll/src/desktop/shell2/common/event.rs`](../../../../dll/src/desktop/shell2/common/event.rs) |

Hit-testing and scroll dispatch flow into this pipeline; see [Hit Testing and Scrolling](hit-testing.md). VirtualView callbacks also generate events the interpreter sees; see [VirtualView Lazy Loading](virtual-view.md). For the IFrame-specific scroll routing problem, see [IFrame Scroll and Display Lists](iframe-scroll.md).
