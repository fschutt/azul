---
slug: accessibility-backends
title: Accessibility Backends
language: en
canonical_slug: accessibility-backends
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Per-platform accessibility back-ends (UIA, AT-SPI, NSAccessibility) and the common tree they expose.
prerequisites: [code-organization]
tracked_files:
  - core/src/a11y.rs
  - layout/src/managers/a11y.rs
  - dll/src/desktop/shell2/linux/x11/accessibility.rs
  - dll/src/desktop/shell2/macos/accessibility.rs
  - dll/src/desktop/shell2/windows/accessibility.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Accessibility Backends

> **WIP.** Everything in `dll/src/desktop/shell2/*/accessibility.rs` is
> `#[cfg(feature = "a11y")]`. The data model in `core/src/a11y.rs` is stable;
> the platform bridges work but several `AccessibilityInfo` fields
> (`labelled_by`, `described_by`, `is_live_region`) are not yet propagated by
> the manager. Treat this page as the wiring map, not the API contract.

The accessibility pipeline is three layers stacked in dependency order:
[`core/src/a11y.rs`](../../../../core/src/a11y.rs) holds the FFI-safe data
types, [`layout/src/managers/a11y.rs`](../../../../layout/src/managers/a11y.rs)
builds an [`accesskit::TreeUpdate`](https://crates.io/crates/accesskit) from
the layout result, and the per-OS bridge under
`dll/src/desktop/shell2/{linux/x11,macos,windows}/accessibility.rs` hands the
update to the platform-specific `accesskit_*` adapter. Action requests flow
back up the stack.

```
core::a11y::AccessibilityInfo  (FFI-safe data)
        │
        ▼
layout::managers::a11y::A11yManager::update_tree(...)
        │  builds accesskit::TreeUpdate
        ▼
dll::desktop::shell2::*::*AccessibilityAdapter::update_tree(tu)
        │
        ▼  per-OS:
        ├─ Linux:   accesskit_unix::Adapter        → AT-SPI (D-Bus)
        ├─ macOS:   accesskit_macos::SubclassingAdapter → NSAccessibility
        └─ Windows: accesskit_windows::SubclassingAdapter → UIA
```

## Data model: `AccessibilityInfo`

[`core/src/a11y.rs:27`](../../../../core/src/a11y.rs) defines the per-node
record. `#[repr(C)]`, FFI-safe, `Hash`:

```rust,ignore
#[repr(C)]
pub struct AccessibilityInfo {
    pub accessibility_name: OptionString,
    pub accessibility_value: OptionString,
    pub description: OptionString,
    pub accelerator: OptionVirtualKeyCodeCombo,
    pub default_action: OptionString,
    pub states: AccessibilityStateVec,
    pub supported_actions: AccessibilityActionVec,
    pub labelled_by: OptionDomNodeId,
    pub described_by: OptionDomNodeId,
    pub role: AccessibilityRole,
    pub is_live_region: bool,
}
```

The lighter-weight constructor for the common case is
[`SmallAriaInfo`](../../../../core/src/a11y.rs)
([`a11y.rs:764`](../../../../core/src/a11y.rs)) — `label`, `role`,
`description`. It expands to `AccessibilityInfo` via
`SmallAriaInfo::to_full_info()`.

The non-self-explanatory fields:

| field | maps to |
|---|---|
| `default_action` | accesskit `Action::Default` description, only meaningful when at least one `ComponentEventFilter::DefaultAction` callback exists on the node |
| `labelled_by` / `described_by` | `aria-labelledby` / `aria-describedby`. Defined but not yet read by the manager |
| `is_live_region` | accesskit `Live` property. Defined but not yet read |
| `supported_actions` | `Vec<AccessibilityAction>` — see below |
| `states` | `Vec<AccessibilityState>` — see below |

## Roles

[`AccessibilityRole`](../../../../core/src/a11y.rs) at
[`a11y.rs:156`](../../../../core/src/a11y.rs) is a `#[repr(C)]` enum modelled
after MSAA / IAccessible role constants. It is wider than accesskit's `Role`
enum — the manager collapses several variants:

```rust,ignore
// layout/src/managers/a11y.rs:609
fn map_role(role: &AccessibilityRole) -> accesskit::Role {
    match role {
        AccessibilityRole::TitleBar      => Role::TitleBar,
        AccessibilityRole::PushButton    => Role::Button,
        AccessibilityRole::CheckButton   => Role::CheckBox,
        AccessibilityRole::ComboBox      => Role::ComboBox,
        AccessibilityRole::Outline       => Role::Tree,
        AccessibilityRole::Column        => Role::GenericContainer, // no Column in accesskit 0.17
        AccessibilityRole::ButtonMenu    => Role::Button,           // no MenuButton in 0.17
        AccessibilityRole::Equation      => Role::Math,
        // ...
        AccessibilityRole::Unknown       => Role::Unknown,
        AccessibilityRole::Nothing       => Role::GenericContainer,
        _ => /* see source */
    }
}
```

The full table is at [`layout/src/managers/a11y.rs:609`](../../../../layout/src/managers/a11y.rs).
When you add a new role to `core::a11y`, you must add a `match` arm here
or screen readers receive `Role::Unknown`.

## States

[`AccessibilityState`](../../../../core/src/a11y.rs) at
[`a11y.rs:608`](../../../../core/src/a11y.rs) — focus, selection, expansion,
checkboxes, etc.:

```rust,ignore
#[repr(C)]
pub enum AccessibilityState {
    Unavailable, Selected, Focused,
    CheckedTrue, CheckedFalse,
    Readonly, Default,
    Expanded, Collapsed,
    Busy, Offscreen,
    Focusable, Selectable,
    Linked, Traversed, Multiselectable,
    Protected,
}
```

The `Vec<AccessibilityState>` carries multiple states per node (e.g., a
focused, focusable, selectable list item). The manager expands flags
individually onto the accesskit `Node` (`set_focused`, `set_selected`, ...).

## Actions

[`AccessibilityAction`](../../../../core/src/a11y.rs) at
[`a11y.rs:66`](../../../../core/src/a11y.rs) is a `#[repr(C, u8)]` superset of
`accesskit::Action` plus payload-carrying variants:

```rust,ignore
#[repr(C, u8)]
pub enum AccessibilityAction {
    Default, Focus, Blur,
    Collapse, Expand, ScrollIntoView,
    Increment, Decrement,
    ShowContextMenu, HideTooltip, ShowTooltip,
    ScrollUp, ScrollDown, ScrollLeft, ScrollRight,
    ReplaceSelectedText(AzString),
    ScrollToPoint(LogicalPosition),
    SetScrollOffset(LogicalPosition),
    SetTextSelection(TextSelectionStartEnd),
    SetSequentialFocusNavigationStartingPoint,
    SetValue(AzString),
    SetNumericValue(FloatValue),
    CustomAction(i32),
}
```

`AccessibilityInfo.supported_actions` is the *list of actions advertised to
the AT*. When the AT performs one, accesskit returns the action via its
`ActionHandler`; the manager translates it back to an
`AccessibilityAction` via
[`map_accesskit_action`](../../../../layout/src/managers/a11y.rs)
([`layout/src/managers/a11y.rs:715`](../../../../layout/src/managers/a11y.rs))
and dispatches it as a synthetic event.

## The manager: `A11yManager`

[`layout/src/managers/a11y.rs:50`](../../../../layout/src/managers/a11y.rs)
holds per-window state:

```rust,ignore
#[cfg(feature = "a11y")]
pub struct A11yManager {
    pub root_id: A11yNodeId,
    pub tree: Option<Tree>,
    pub last_tree_update: Option<TreeUpdate>,
    pub tree_initialized: bool,
}
```

The two entry points:

```rust,ignore
// Build a TreeUpdate from a layout result.
pub fn update_tree(
    root_id: A11yNodeId,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    window_title: &AzString,
    window_size: LogicalSize,
    focused_node: Option<DomNodeId>,
    hidpi_factor: f32,
    dirty_text_overrides: &BTreeMap<(DomId, NodeId), String>,
    cursor_info: Option<CursorA11yInfo>,
) -> TreeUpdate;

// Decode an action request from the AT.
pub fn handle_action_request(
    &self,
    request: ActionRequest,
) -> Option<(DomNodeId, AccessibilityAction)>;
```

`update_tree` walks every `DomLayoutResult`, allocates an `accesskit::Node`
per laid-out element, sets role / label / bounds / state, and stitches them
into the `TreeUpdate`. The `A11yNodeId` for an Azul node is encoded as:

```text
upper 32 bits = DomId
lower 32 bits = NodeId + 1   (0 is reserved for the root window)
```

`handle_action_request` reverses that encoding
([`layout/src/managers/a11y.rs:697`](../../../../layout/src/managers/a11y.rs))
and returns a `(DomNodeId, AccessibilityAction)` the event system can dispatch.

`tree_initialized` flips `false → true` after the first full tree push so
later updates can omit the `tree` field — accesskit treats absent `tree` as
"node-set delta only".

`CursorA11yInfo` is an out-of-band channel for text selection: when the user
moves the caret in a `contenteditable` node, the manager attaches
`text_selection` to that node so screen readers can announce cursor position
without a full tree rebuild.

## The Linux bridge — AT-SPI via `accesskit_unix`

[`dll/src/desktop/shell2/linux/x11/accessibility.rs`](../../../../dll/src/desktop/shell2/linux/x11/accessibility.rs).
Same module is used for Wayland (the adapter does not care about the
display protocol; it talks D-Bus to AT-SPI directly):

```rust,ignore
#[cfg(feature = "a11y")]
pub struct LinuxAccessibilityAdapter {
    adapter: Arc<Mutex<Option<Adapter>>>,
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}
```

Lifecycle:

| Step | What happens |
|---|---|
| `LinuxAccessibilityAdapter::new()` | allocates the mutexes, defers adapter construction |
| `initialize(window_name)` | builds an `accesskit_unix::Adapter` inside `panic::catch_unwind` (D-Bus connection failures must not crash the app) |
| `update_tree(tree_update)` | `try_lock` (never blocks the UI), then `adapter.update_if_active(\|\| tree_update)` |
| AT triggers an action | `accesskit_unix` calls `ActionHandler::do_action`, which pushes to `pending_actions`. The event loop drains and feeds them back to `A11yManager::handle_action_request` |
| `set_focus(_)` | no-op — focus state is managed by `accesskit_unix` itself |

`update_if_active` is the load-bearing call: if the AT is not currently
listening, the closure is never invoked and no D-Bus traffic is generated.

## The macOS bridge — NSAccessibility via `accesskit_macos`

[`dll/src/desktop/shell2/macos/accessibility.rs`](../../../../dll/src/desktop/shell2/macos/accessibility.rs):

```rust,ignore
#[cfg(feature = "a11y")]
pub struct MacOSAccessibilityAdapter {
    adapter: SubclassingAdapter,
    action_receiver: Receiver<ActionRequest>,
    tree_provider: Arc<Mutex<Option<TreeUpdate>>>,
}
```

Constructed with `MacOSAccessibilityAdapter::new(view: *mut c_void)` —
`view` is the raw `NSView` pointer the platform window owns.
`SubclassingAdapter` rewrites a few `NSObject` methods on that view to make
it conform to `NSAccessibilityProtocol`.

`tree_provider` is a `Mutex<Option<TreeUpdate>>`. The activation handler
([`accessibility.rs:23`](../../../../dll/src/desktop/shell2/macos/accessibility.rs))
**returns `None`** the first time `request_initial_tree` is called. This is
deliberate:

> Returning `Some` here would skip Placeholder and go directly Inactive →
> Active, which does NOT generate focus events.
>
> ([`accessibility.rs:30-36`](../../../../dll/src/desktop/shell2/macos/accessibility.rs))

VoiceOver depends on the Placeholder → Active transition firing
`AXFocusedUIElementChanged`. The first real `update_tree` call promotes the
adapter into Active state with focus events intact.

Action requests flow through an `mpsc::channel` rather than a mutex-guarded
vec — the macOS AT may invoke action handlers off the main thread, and the
event loop drains the receiver each frame.

## The Windows bridge — UIA via `accesskit_windows`

[`dll/src/desktop/shell2/windows/accessibility.rs`](../../../../dll/src/desktop/shell2/windows/accessibility.rs):

```rust,ignore
#[cfg(feature = "a11y")]
pub struct WindowsAccessibilityAdapter {
    adapter: Arc<Mutex<Option<SubclassingAdapter>>>,
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}
```

`initialize(hwnd)` constructs an `accesskit_windows::SubclassingAdapter` —
this hooks the `WM_GETOBJECT` message on the HWND so when UIA queries
`OBJID_CLIENT`, the adapter responds. Wrapped in `catch_unwind` so a UIA
panic cannot crash the app.

`update_tree` uses `try_lock` for the same non-blocking reason as Linux.
Pending actions buffer in a `Mutex<Vec<ActionRequest>>` and the event loop
drains them.

## Common backend invariants

All three adapters share these properties — when adding a new backend, match
them:

- **`#[cfg(feature = "a11y")]` everywhere.** A no-op stub must compile when
  the feature is off.
- **`try_lock`, never `lock`.** Skipping an a11y update is preferable to
  freezing the UI.
- **`catch_unwind` around adapter construction and tree pushes.** AT
  middleware (D-Bus, UIA, NSAccessibility) is not part of azul's trust
  boundary — panics in third-party code must not propagate.
- **Action requests buffered, drained by the event loop.** Adapters are
  often called from AT-owned threads.

## What is unwired

Three fields on `AccessibilityInfo` are stored but not consumed by
`A11yManager::update_tree`:

- `labelled_by`, `described_by` — should set
  `accesskit::Node::push_labelled_by` / `push_described_by`.
- `is_live_region` — should set `accesskit::Node::set_live`.

`SmallAriaInfo::label` is not called from any Rust source in this repository
— it exists for the C/C++/Python FFI surface (search `api.json` for
`SmallAriaInfo.label` to confirm).

`MenuItemIcon::Image` rendering inside menu DOMs is also unwired (see
[Menus and Client-Side Decorations](./menus-and-csd.md)) — relevant here
because menus are part of the accessibility tree and missing icons may show
up as unlabelled images to screen readers.
