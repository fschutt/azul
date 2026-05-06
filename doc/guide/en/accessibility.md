---
slug: accessibility
title: Accessibility
language: en
canonical_slug: accessibility
audience: external
maturity: wip
guide_order: 280
topic_only: false
short_desc: Screen reader integration and ARIA roles
prerequisites: [dom]
tracked_files:
  - core/src/a11y.rs
  - layout/src/managers/a11y.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Accessibility

> **WIP.** Accessibility is opt-in behind the `a11y` Cargo feature. The
> user-facing APIs described here are stable.

Azul exposes accessibility metadata to assistive technologies (screen readers,
switch navigation, voice control). You attach an `AccessibilityInfo` to a
node, and the framework publishes it on the next layout pass.

```rust,no_run
# use azul::prelude::*;
let save = Dom::create_button(
    "Save",
    SmallAriaInfo::label("Save the document".into())
        .with_role(AccessibilityRole::PushButton),
);
```

There are two entry points. `SmallAriaInfo` covers the common case (label,
role, description). `AccessibilityInfo` carries the full record (states,
actions, accelerators, live regions, label/describe-by relationships).

## Adding a label, role, and description with SmallAriaInfo

`SmallAriaInfo` is a builder over three optional fields:

```rust,ignore
pub struct SmallAriaInfo {
    pub label: OptionString,
    pub role: OptionAccessibilityRole,
    pub description: OptionString,
}
```rust

Construct with `SmallAriaInfo::label(text)` and chain `with_role` and
`with_description`:

```rust,no_run
# use azul::prelude::*;
let aria = SmallAriaInfo::label("Submit form".into())
    .with_role(AccessibilityRole::PushButton)
    .with_description("Sends the form to the server.".into());
```rust

`label` is the accessible name screen readers announce. `role` overrides the
role that would otherwise be inferred from the HTML tag. `description` is read
after the label as supplementary context. Use it sparingly. It does not
replace a missing label.

## Constructors that require accessibility info

The accessible variants of the `Dom::create_*` helpers take a `SmallAriaInfo`
parameter. Each interactive element comes in two forms.

- `Dom::create_button(text, aria)`, with the escape hatch `create_button_no_a11y(text)`.
- `Dom::create_a(href, text, aria)`, with the escape hatch `create_a_no_a11y(href, text)`.
- `Dom::create_input(ty, name, label, aria)`, with the escape hatch
  `create_input_no_a11y(ty, name, label)`.
- `Dom::create_textarea(name, label, aria)`, with the escape hatch
  `create_textarea_no_a11y(name, label)`.
- `Dom::create_select(name, label, aria)`, with the escape hatch
  `create_select_no_a11y(name, label)`.
- `Dom::create_table(caption, aria)`, with the escape hatch `create_table_no_a11y()`.
- `Dom::create_label(for_id, text, aria)`, with the escape hatch
  `create_label_no_a11y(for_id, text)`.

The `_no_a11y` variants are deliberately verbose. Use them only when the
accessible name comes from somewhere else, for example an icon-only link
whose name comes from a sibling image's alt text.

For elements you build with `Dom::create_div().with_child(...)`, attach a label
through `Dom::with_accessibility_info(AccessibilityInfo)`.

## Going beyond labels: AccessibilityInfo

`AccessibilityInfo` is the full record:

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

Set this when you need any of: dynamic states, custom action lists, keyboard
accelerators, label-by/describe-by relationships, or live regions.

## AccessibilityRole: what kind of element this is

`AccessibilityRole` is an enum over the standard role set. Most HTML tags are
mapped automatically. You set `role` only when the visual element does not
match its tag, for example a div styled as a tab strip:

```rust,no_run
# use azul::prelude::*;
let tab = SmallAriaInfo::label("Settings tab".into())
    .with_role(AccessibilityRole::PageTab);
```

Roles commonly used in app code:

- `PushButton` for a clickable button.
- `Link` for a hyperlink.
- `CheckButton` for a checkbox.
- `RadioButton` for a radio button.
- `ComboBox` for a dropdown with text input.
- `DropList` for a dropdown without text input.
- `Slider` for a slider.
- `SpinButton` for a numeric stepper.
- `ProgressBar` for a progress indicator.
- `MenuItem` for a menu item.
- `PageTab` and `PageTabList` for an individual tab and its strip.
- `Outline` and `OutlineItem` for a tree view and its nodes.
- `List` and `ListItem` for a list and its items.
- `Table`, `Row`, and `Cell` for a data grid.
- `Dialog` for a modal dialog.
- `Alert` for a non-modal notification.
- `Tooltip` for a hover tooltip.

`Unknown` is the default fallback and tells the platform to infer from the
tag. `Nothing` explicitly hides the element from assistive tech.

## AccessibilityState: dynamic state

States live on `AccessibilityInfo.states`. Push them whenever the visual state
changes. A toggled checkbox flips `CheckedFalse` to `CheckedTrue`. A busy
panel adds `Busy` until loading finishes.

- `Unavailable`: disabled or grayed out.
- `Selected`: item is selected (independent of focus).
- `Focused`: has keyboard focus (managed automatically).
- `CheckedTrue` and `CheckedFalse`: binary toggle.
- `Readonly`: not editable.
- `Default`: the default action button in a dialog.
- `Expanded` and `Collapsed`: disclosure state.
- `Busy`: element is unresponsive while working.
- `Offscreen`: scrolled out of view.
- `Focusable`: can receive focus.
- `Selectable` and `Multiselectable`: container supports selection.
- `Linked` and `Traversed`: hyperlink, possibly visited.
- `Protected`: sensitive content (passwords).

State changes only become visible to assistive tech after the next layout pass
rebuilds the a11y tree. Return `Update::RefreshDom` from your callback when
you mutate states so the new tree is shipped.

## AccessibilityAction: what assistive tech can do

Actions populate `AccessibilityInfo.supported_actions`. The platform routes
incoming requests back to your DOM node:

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

For most elements you don't need to list actions explicitly. The framework
adds `Focus` for focusable nodes, `Click` for nodes with activation behavior,
and the text-editing actions (`SetTextSelection`, `ReplaceSelectedText`,
`SetValue`) for inputs and contenteditable regions. List actions in
`supported_actions` only when you implement custom behavior, for example
`Increment` and `Decrement` on a div-based slider.

## Live regions

Set `AccessibilityInfo.is_live_region = true` on an element whose content
changes while the user is doing something else. Screen readers announce
live-region updates without requiring focus. Typical uses are chat windows,
toast notifications, autosave indicators, and timers.

## Headings

Headings use the standard h1 through h6 tags. The framework maps them to the
accessibility tree's heading role with the level set, so screen-reader heading
navigation works without per-element configuration. Use real heading tags
rather than styled divs.

## Focus and text editing

The focus manager (covered in `events`) tracks the focused node. The a11y
tree's focus follows that value so assistive tech moves with the keyboard.

For contenteditable, input, and textarea elements:

- The current text is exposed as the node's value, not its label, so screen
  readers can announce edits without re-reading the field name.
- The text-editing actions (`SetTextSelection`, `ReplaceSelectedText`,
  `SetValue`) are added automatically.
- When the focused element has an active selection or cursor, the tree
  reports character lengths and anchor/focus positions so the screen reader
  can announce the cursor position or selection length.

## Action requests are events

When assistive tech invokes an action, the framework synthesizes the matching
native event:

- `Default`, `Focus`, `Blur` become focus and click events to your callbacks.
- Scroll actions adjust the scroll offset of the matching scroll frame.
- `SetValue`, `ReplaceSelectedText`, and `SetTextSelection` become text input
  events delivered to the focused contenteditable.
- `CustomAction(i32)` fires whatever event filter your DOM declares for the
  default action. Set `AccessibilityInfo.default_action` to a human-readable
  description so the screen reader can announce it.

You don't write separate code paths for assistive tech. The same callback that
handles a mouse click handles `AccessibilityAction::Default`.

## Enabling the feature

Accessibility is gated behind the `a11y` Cargo feature. With the feature off,
the a11y manager is a no-op. You can still set `SmallAriaInfo` on your DOM
and the data is preserved, but no a11y tree is published.

```toml
# Cargo.toml
[dependencies]
azul = { version = "*", features = ["a11y"] }
```

## Coming Up Next

- [Built-in Widgets](widgets.md) — Built-in widgets and how to write your own
- [System Themes](styling/themes.md) — System colors, `@theme`, `@os`, and accessibility queries
- [Events](events.md) — Callbacks, event filters, and how state triggers relayout
