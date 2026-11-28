---
slug: system/accessibility
title: Accessibility
language: en
canonical_slug: system/accessibility
audience: external
maturity: wip
guide_order: 125
topic_only: false
short_desc: Screen reader integration and ARIA roles
prerequisites: [dom]
tracked_files:
  - core/src/a11y.rs
  - layout/src/managers/a11y.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - AccessibilityInfo
  - SmallAriaInfo
  - ProgressAriaInfo
  - MeterAriaInfo
  - DialogAriaInfo
  - AccessibilityRole
  - AccessibilityAction
  - AccessibilityState
  - Dom
  - NodeType
---

# Accessibility

## Introduction

Azul exposes accessibility metadata to assistive technologies (screen readers,
switch navigation, voice control). The way it works is that you attach an 
`AccessibilityInfo` to a node, and the framework publishes it on the next 
layout pass.

```rust,no_run
use azul::prelude::*;
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
```

Construct with `SmallAriaInfo::label(text)` and chain `with_role` and
`with_description`:

```rust,no_run
use azul::prelude::*;
let aria = SmallAriaInfo::label("Submit form".into())
    .with_role(AccessibilityRole::PushButton)
    .with_description("Sends the form to the server.".into());
```

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
use azul::prelude::*;
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

## The user's display settings

Besides assistive technology, the system holds settings that change how an app
should look: the text size, bold text, increased contrast, reduced motion,
reduced transparency and a few more. Azul reads them when a window starts and
again when they change, and answers the ones it can answer itself.

Every option is a typed state, never a bare boolean: `Unknown` (this platform
has no such option, or it did not answer), `Off` or `On`. A platform that cannot
read an option says `Unknown`; the engine treats it as the default look. The
text size is a number, the contrast a `ContrastPreference` (`NoPreference`,
`More`, `Less`).

### What the engine does for you

| Setting | What happens without any code in the app |
|---|---|
| Text size | Every font size, absolute line height, letter and word spacing is multiplied by the user's scale, once. `em` and `rem` follow. A box sized in `px` keeps its size; a box sized by its text grows with it. |
| Bold text | Every text takes the next heavier weight (400 becomes 700, a 700 stays). |
| Reduced motion | Every declared `animation` (transitions, slides, flips, fades, enter and exit effects) reads as absent: the end state is reached at once. |
| Increased contrast | Every text is read at 7:1 on the surface behind it, an opaque fill a text sits on is one an extreme ink reaches that on, borders and rings (a shadow without blur and offset: a focus halo) are seen at 4.5:1, and a gradient fill is one flat colour. A translucent fill, a soft shadow and a transparent colour stay as written. |
| Reduced transparency | A frosted surface (a translucent fill with a `backdrop-filter`) is an opaque fill and paints no blur. A scrim or a tint, which are not frosted glass, stay as written. |

The text scale is what the system calls it: Android's `fontScale` (and the
non-linear scaling since Android 14), iOS's Dynamic Type category (`Large` is
1.0, `accessibility-extra-extra-extra-large` is about 3.1), Windows' "Text size"
slider (100 to 225 percent), GNOME's `text-scaling-factor`. macOS has no system
text size an app can read. A scale outside 0.5 to 4.0 is capped, with a
diagnostic.

Chrome that must not grow with the text (a rail of icon glyphs, a status bar, a
tab pill) caps the scale for its subtree with a custom property, which
inherits:

```css
.rail { --azul-text-scale-max: 1.3; }
```

The text in `.rail` and below uses the smaller of the user's scale and 1.3. A cap
below 1.0 reads as 1.0 (it limits growth only; text the user made smaller stays
small).

A subtree whose colours ARE the content (a hue strip, the series of a chart, a
map, the frame of a photo) is left as written under increased contrast, with
another custom property that inherits:

```css
.hue-strip { --azul-contrast-adjust: none; }
```

### Reading the settings in CSS

```css
@media (min-text-scale: 1.3) { .toolbar { flex-direction: column; } }
@media (prefers-bold-text: bold) { .badge { letter-spacing: 0.02em; } }
@media (prefers-contrast: more) { .card { border: 2px solid system:text; } }
@media (forced-colors: active) { .chart { border: 1px solid system:text; } }
@media (prefers-reduced-transparency: reduce) { .sheet { background: system:window-background; } }
@media (differentiate-without-color: active) { .status { text-decoration: underline; } }
```

The features: `min-text-scale`, `max-text-scale`, `prefers-bold-text`,
`prefers-contrast` (`more`, `less`, `no-preference`, `custom`), `forced-colors`,
`prefers-reduced-motion`, `prefers-reduced-transparency`, `inverted-colors`,
`differentiate-without-color`, `button-shapes`, `on-off-labels`,
`screen-reader`, `prefers-cross-fade` and `keyboard-navigation`. An option the
platform could not read counts as off in a media query, as in a browser that
does not know the feature: a rule for the `active` side never fires on a guess.
`prefers-contrast: custom` is the same as `forced-colors: active` (the user chose
the palette).

### Reading them in code, and hearing about a change

`LayoutCallbackInfo::get_accessibility_preferences` (in `layout()`) and
`CallbackInfo::get_accessibility_preferences` return the typed preferences the
window lays out under. When the system changes one, the window is restyled
(the retained DOM is re-solved; `layout()` runs again only if it declared that
it reads the style) and a window listener hears it once:

```rust,ignore
dom.with_callback(
    EventFilter::Window(WindowEventFilter::SystemStyleChanged),
    data,
    on_system_style_changed,
)
```

An app that draws on its own (a canvas, a map, a chart) re-reads the
preferences there.

### Trying a setting without touching the system

The same file of a scenario runs on a desktop, an emulator and a phone:

```json
{ "op": "set_system_accessibility", "text_scale": 1.3, "bold_text": true }
{ "op": "set_system_accessibility", "text_scale": "accessibility-extra-extra-extra-large" }
{ "op": "get_system_accessibility" }
{ "op": "set_system_accessibility", "reset": true }
```

`text_scale` takes a factor, an iOS category (`large`, `ax5`, `accessibility-extra-extra-extra-large`
and the `simctl` names) or `null`; the other keys are the option names (`bold_text`,
`contrast`, `reduced_motion`, `reduced_transparency`, ...) with `true`, `false`,
`"on"`, `"off"`, `"unknown"`. `reset: true` puts back what the platform says.
The window's override is its own: it does not change the system.

For a whole process, the environment pins an option, whatever the platform
reports: `AZ_A11Y_TEXT_SCALE=1.5`, `AZ_A11Y_BOLD_TEXT=1`, `AZ_A11Y_CONTRAST=more`,
`AZ_A11Y_REDUCED_MOTION=on`, and so on (one variable per option, named after it).

### Where each setting comes from

| Platform | Source | Change |
|---|---|---|
| Android | `Configuration.fontScale`, `fontWeightAdjustment`, `Settings.Global` / `Secure` / `System`, `AccessibilityManager`, `UiModeManager.getContrast`, `CaptioningManager` | configuration change, resume, and a read once a second |
| iOS | `UIApplication.preferredContentSizeCategory`, the `UIAccessibility*` flag functions, `AXShowBordersEnabled`, `MADimFlashingLightsEnabled` | read four times a second in the display tick |
| macOS | `NSWorkspace` display and accessibility flags | the workspace poll |
| Windows | `SystemParametersInfo`, the registry (`TextScaleFactor`, transparency, pointer size, colour filters) | `WM_SETTINGCHANGE`, and a read once a second |
| Linux | GNOME `org.gnome.desktop.interface` and `a11y` schemas (text scaling factor, high contrast, `reduced-motion`, `show-status-shapes`, cursor size, screen reader), the settings portal's `contrast` and `reduced-motion`, KDE `kdeglobals` (animation speed) | a thread of each window listens for the portal's, dconf's and KConfig's announcements and reads again; a read every two minutes catches the rest |

## Enabling the feature

Accessibility is gated behind the `a11y` Cargo feature. With the feature off,
the a11y manager is a no-op. You can still set `SmallAriaInfo` on your DOM
and the data is preserved, but no a11y tree is published.

```toml
# Cargo.toml
[dependencies]
azul = { version = "*", features = ["a11y"] }
```
