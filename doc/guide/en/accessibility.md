---
slug: accessibility
title: Accessibility
language: en
canonical_slug: accessibility
audience: external
maturity: wip
guide_order: 280
topic_only: false
prerequisites: [dom]
tracked_files:
  - core/src/a11y.rs
  - layout/src/managers/a11y.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T17:30:00Z
---

> **WIP.** Accessibility is opt-in behind the `a11y` Cargo feature.
> macOS and Windows back-ends ship; the Linux/AT-SPI back-end is in
> progress. The user-facing APIs described here are stable.

Azul exposes accessibility metadata to assistive technologies (screen
readers, switch navigation, voice control) via the
[accesskit](https://docs.rs/accesskit) tree:

```rust,no_run
# use azul_core::dom::Dom;
# use azul_core::a11y::{SmallAriaInfo, AccessibilityRole};
let save = Dom::create_button(
    "Save",
    SmallAriaInfo::label("Save the document")
        .with_role(AccessibilityRole::PushButton),
);
```

Each `NodeData` carries an optional `Box<AccessibilityInfo>`; after
layout, `A11yManager` walks the styled DOM, builds an
`accesskit::TreeUpdate`, and ships it to the platform adapter
(NSAccessibility on macOS, UIA on Windows, AT-SPI on Linux). The two
entry points are `SmallAriaInfo` for the common case (label + role +
description) and `AccessibilityInfo` for the full metadata record
(states, actions, accelerators, live regions, label/describe-by
relationships).

## Adding a label, role, and description with `SmallAriaInfo`

`SmallAriaInfo` is a builder over three optional fields:

```rust,ignore
pub struct SmallAriaInfo {
    pub label: OptionString,
    pub role: OptionAccessibilityRole,
    pub description: OptionString,
}
```

Construct with `SmallAriaInfo::label(text)` and chain `with_role` /
`with_description`:

```rust,no_run
# use azul_core::a11y::{SmallAriaInfo, AccessibilityRole};
let aria = SmallAriaInfo::label("Submit form")
    .with_role(AccessibilityRole::PushButton)
    .with_description("Sends the form to the server.");
```

`label` is the accessible name screen readers announce. `role` overrides
the role that would otherwise be inferred from the HTML tag.
`description` is read after the label as supplementary context — use it
sparingly; it does not replace a missing label.

## Constructors that require accessibility info

The accessible variants of the `Dom::create_*` helpers take a
`SmallAriaInfo` parameter. Each interactive element comes in two forms:

| Element | Required label | Escape hatch |
|---|---|---|
| `Dom::create_button(text, aria)` | yes | `create_button_no_a11y(text)` |
| `Dom::create_a(href, text, aria)` | yes | `create_a_no_a11y(href, text)` |
| `Dom::create_input(ty, name, label, aria)` | yes | `create_input_no_a11y(ty, name, label)` |
| `Dom::create_textarea(name, label, aria)` | yes | `create_textarea_no_a11y(name, label)` |
| `Dom::create_select(name, label, aria)` | yes | `create_select_no_a11y(name, label)` |
| `Dom::create_table(caption, aria)` | yes | `create_table_no_a11y()` |
| `Dom::create_label(for_id, text, aria)` | yes | `create_label_no_a11y(for_id, text)` |

The `_no_a11y` variants are deliberately verbose. Use them only when
the accessible name comes from somewhere else — for example, an
icon-only link whose name comes from a sibling `<img alt>`.

```rust,no_run
# use azul_core::dom::Dom;
# use azul_core::a11y::{SmallAriaInfo, AccessibilityRole};
let save = Dom::create_button(
    "Save",
    SmallAriaInfo::label("Save the document")
        .with_role(AccessibilityRole::PushButton),
);
```

For elements you build with `Dom::div().with_child(...)`, attach a label
through `Dom::with_accessibility_info(AccessibilityInfo)` or convert
from `SmallAriaInfo` via `aria.to_full_info()`.

## Going beyond labels: `AccessibilityInfo`

`AccessibilityInfo` (`core/src/a11y.rs:23`) is the full record:

```rust,ignore
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

Set this when you need any of: dynamic states, custom action lists,
keyboard accelerators, label-by/describe-by relationships, or live
regions. `SmallAriaInfo::to_full_info()` produces a sane default record
with empty state and action vectors.

## `AccessibilityRole` — what kind of element this is

`AccessibilityRole` is an enum over the standard MSAA / IA2 role set
(`core/src/a11y.rs:138`). Most HTML tags are mapped automatically — you
need to set `role` only when the visual element does not match its tag,
e.g. a `<div>` styled as a tab strip:

```rust,no_run
# use azul_core::a11y::{SmallAriaInfo, AccessibilityRole};
let tab = SmallAriaInfo::label("Settings tab")
    .with_role(AccessibilityRole::PageTab);
```

The most common roles used by app code:

| Role | Element |
|---|---|
| `PushButton` | clickable button |
| `Link` | hyperlink |
| `CheckButton` | checkbox |
| `RadioButton` | radio button |
| `ComboBox` | dropdown with text input |
| `DropList` | dropdown without text input |
| `Slider` | slider |
| `SpinButton` | numeric stepper |
| `ProgressBar` | progress indicator |
| `MenuItem` | menu item |
| `PageTab` / `PageTabList` | individual tab / tab strip |
| `Outline` / `OutlineItem` | tree view / tree node |
| `List` / `ListItem` | list / item |
| `Table` / `Row` / `Cell` | data grid |
| `Dialog` | modal dialog |
| `Alert` | non-modal notification |
| `Tooltip` | hover tooltip |

For the full list see `core/src/a11y.rs:138`.

The `Unknown` and `Nothing` roles have specific meanings: `Unknown` is
the default fallback and tells the platform to infer from the tag;
`Nothing` explicitly hides the element from assistive tech.

## `AccessibilityState` — dynamic state

States live on `AccessibilityInfo::states` and are pushed by your code
whenever the visual state changes. A toggled checkbox flips
`CheckedFalse → CheckedTrue`; a busy panel adds `Busy` until loading
finishes.

| State | Meaning |
|---|---|
| `Unavailable` | disabled / grayed out |
| `Selected` | item is selected (independent of focus) |
| `Focused` | has keyboard focus (managed automatically) |
| `CheckedTrue` / `CheckedFalse` | binary toggle |
| `Readonly` | not editable |
| `Default` | the default action button in a dialog |
| `Expanded` / `Collapsed` | disclosure state |
| `Busy` | element is unresponsive while working |
| `Offscreen` | scrolled out of view |
| `Focusable` | can receive focus |
| `Selectable` / `Multiselectable` | container supports selection |
| `Linked` / `Traversed` | hyperlink, possibly visited |
| `Protected` | sensitive content (passwords) |

State changes only become visible to assistive tech after the next
layout pass rebuilds the a11y tree. Return
`Update::RefreshDom` from your callback when you mutate states so the
new tree is shipped.

## `AccessibilityAction` — what assistive tech can do

Actions are populated on `AccessibilityInfo::supported_actions`. The
platform adapter routes incoming `ActionRequest`s back to your DOM
node:

```rust,ignore
pub enum AccessibilityAction {
    Default, Focus, Blur,
    Collapse, Expand,
    ScrollIntoView,
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

For most elements you do not need to list actions explicitly.
[`A11yManager::build_node`] adds `Focus` for focusable nodes,
`Click` for nodes with activation behavior, and the text-editing
trio (`SetTextSelection`, `ReplaceSelectedText`, `SetValue`) for inputs
and `contenteditable` regions. List actions in `supported_actions`
only when you implement custom behavior — for example, `Increment` /
`Decrement` on a `<div>`-based slider.

[`A11yManager::build_node`]: #how-the-tree-is-built

## How the tree is built

After every layout pass that produces a new styled DOM, the layout
window calls `A11yManager::update_tree`
(`layout/src/managers/a11y.rs:79`) with:

- the per-DOM layout results (positioned rects),
- the current window title and viewport size,
- the focused `DomNodeId`, if any,
- a map of dirty contenteditable text overrides,
- optional cursor / selection info for the focused text input.

`update_tree` walks every `NodeData`. Nodes are included if they
declare `AccessibilityInfo`, are focusable, are `contenteditable`, or
have a node type that screen readers care about — metadata tags
(`<head>`, `<meta>`, `<style>`, `<script>`, etc.) and pseudo-elements
are skipped. For each included node it calls `build_node`, which:

1. Picks a role: `MultilineTextInput` for `contenteditable`, otherwise
   the `AccessibilityInfo::role` (mapped through `map_role` to
   accesskit), otherwise `node_type_to_role` for the HTML tag.
2. Sets the accessible name from (in priority order)
   `AccessibilityInfo::accessibility_name`, the `aria-label` HTML
   attribute, the `<title>` / `<alt>` attributes, or the text content
   of pure-text descendants.
3. Pushes states from `AccessibilityInfo::states` plus implicit states
   from HTML attributes (`disabled`, `readonly`, `checked`, `required`,
   `hidden`, `lang`, `colspan`, `rowspan`).
4. Sets bounds from the layout result, scaled to physical pixels and
   clipped to the viewport so VoiceOver highlights do not overshoot
   off-screen.
5. Adds implicit actions (`Focus`, `Click`) where the DOM warrants
   them.

The returned `accesskit::TreeUpdate` is stored on the manager and
consumed by the platform adapter on the next event tick.

## When the layout updates

The accessibility tree is rebuilt on every full layout pass. Layout
runs when:

- A callback returns `Update::RefreshDom`.
- The window resizes.
- A scroll changes which nodes are in view (the scroll path updates
  the tree's node bounds without rebuilding the tree).

For micro-updates that do not invalidate layout — the user typing into
a `contenteditable` — the relayout path collects dirty text into a
side map and feeds it to `update_tree` so the screen reader sees
current characters even before a full re-layout.

## Live regions

Set `AccessibilityInfo::is_live_region = true` on an element whose
content changes while the user is doing something else. Screen readers
announce live-region updates without requiring focus. Typical uses:
chat windows, toast notifications, autosave indicators, timers.

```rust,no_run
# use azul_core::a11y::{AccessibilityInfo, AccessibilityRole, SmallAriaInfo};
# use azul_css::OptionString;
let mut info = SmallAriaInfo::label("Status: 3 unread")
    .with_role(AccessibilityRole::Alert)
    .to_full_info();
info.is_live_region = true;
```

## Heading levels

Headings use the standard `<h1>` … `<h6>` tags. The a11y manager maps
them to `accesskit::Role::Heading` with the level set
(`layout/src/managers/a11y.rs:408`), so screen-reader heading
navigation (VoiceOver `Ctrl+Cmd+H`, NVDA `H`) works without per-element
configuration. Use real heading tags rather than styled `<div>`s.

## Focus and text editing

The `FocusManager` (covered later in `events`) tracks the focused
`DomNodeId`. The a11y tree's `focus` field follows that value so
assistive tech moves with the keyboard.

For `contenteditable` and `<input>`/`<textarea>`:

- The current text is exposed as the node's value, not its label, so
  screen readers can announce edits without re-reading the field name.
- The text-editing actions (`SetTextSelection`, `ReplaceSelectedText`,
  `SetValue`) are added automatically.
- When the focused element has an active selection or cursor,
  `update_tree` reports character lengths and anchor/focus positions
  so the screen reader can announce "selected three characters" or
  cursor position.

## Handling action requests

When assistive tech invokes an action, the platform adapter calls
`A11yManager::handle_action_request` (`layout/src/managers/a11y.rs:685`)
which decodes the target into a `(DomNodeId, AccessibilityAction)`
pair. The shell synthesizes the matching native event:

- `Default` / `Focus` / `Blur` → focus + click events to your callbacks.
- Scroll actions → adjust the scroll offset of the matching scroll
  frame.
- `SetValue` / `ReplaceSelectedText` / `SetTextSelection` → text input
  events delivered to the focused contenteditable.
- `CustomAction(i32)` → fires whatever event filter your DOM declares
  for `ComponentEventFilter::DefaultAction` (set
  `AccessibilityInfo::default_action` to a human-readable description
  so VoiceOver can announce it).

Because action requests are translated to ordinary events, you do not
write separate code paths for assistive tech — the same callback that
handles a mouse click handles `AccessibilityAction::Default`.

## Enabling the feature

Accessibility is gated behind the `a11y` Cargo feature. With the
feature off, `A11yManager` is a no-op stub and platform adapters
compile to empty shims (`layout/src/managers/a11y.rs:790`,
`dll/src/desktop/shell2/macos/accessibility.rs:6`). You still set
`SmallAriaInfo` on your DOM — the data is preserved — but no a11y
tree is published.

```toml
# Cargo.toml
[dependencies]
azul = { version = "*", features = ["a11y"] }
```

For Linux you also need the AT-SPI back-end, which is still in
progress. Until it lands, Linux builds with `a11y` enabled compile
but expose no a11y tree.

## Platform support

| Platform | Bridge | Crate | Status |
|---|---|---|---|
| macOS | NSAccessibility | `accesskit_macos` | shipped |
| Windows | UI Automation (UIA) | `accesskit_windows` | shipped |
| Linux X11/Wayland | AT-SPI | `accesskit_unix` | wip |

VoiceOver, NVDA, and JAWS are the primary test targets. Test scripts
in `scripts/` exercise the a11y tree on macOS via `osascript`.
