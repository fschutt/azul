# Debugger UI Requirements

## 1. CSS Properties Layout
- CSS property keys and values should be on **opposite sides** of the flex container (key left, value right, `justify-content: space-between`)
- The CSS properties section should be its own **scrollable container** with a fixed max-height, since properties lists get very long — the rest of the detail panel should not scroll with them
- The `border-bottom` on hover/edit of CSS property values causes a **layout shift** — fix by adding a transparent border by default so the shift doesn't happen

## 2. Accessibility & Clip Mask Inspection
- Add a **non-default Accessibility** section to the node detail panel, showing a11y info (role, label, tab-index, contenteditable, etc.)
- Add a **Clip Mask** inspection section, to see and debug clip/scroll nesting for the selected node (uses `get_display_list` clip_analysis data)

## 3. Terminal Image Display
- When the terminal receives a response containing a base64-encoded PNG (e.g. from `take_screenshot` or `take_native_screenshot`), display the image **inline in the terminal/debug console** as an `<img>` element
- Let the browser decode the base64 PNG directly — no processing needed

## 4. Slash Command Popup Improvements
- The autocomplete popup should show:
  - Command name (already shown)
  - **Description** text (already shown)
  - **Example usage** with named-parameter syntax, e.g. `/double_click x 200 y 300`
  - For commands with **multiple targeting variants** (e.g. `click` supports `selector`, `text`, `node_id`, `x`/`y`), show **multiple example lines** in a smaller font below the command name
- The slash command syntax should be **named parameters**: `/double_click x 200 y 300` instead of positional `/double_click 200 300`
- Parse named params in `_parseSlashCommand`: detect `key value` pairs after the command name

## 5. Test Explorer Redesign
- Remove the large blue "Run" / "All" buttons from the sidebar
- The sidebar test list should show **editable test names** in-place
- Each test in the list should be selectable; when selected, its steps appear in the main editor area
- Step items should be **compact**: if a step has no parameters, don't show a "No params" line or extra metadata — just the operation name
- Step items with parameters should show them inline (compact), e.g. `click selector=".btn"`

## 6. Add Step Toolbar
- The "Add Step" button should be in a **toolbar row** below the "runner.e2e" tab title, left-aligned, together with the play/pause/step-over/reset buttons — NOT floating, NOT in a separate bottom bar
- The toolbar should look like a compact icon bar (similar to VS Code's debug toolbar)
- Clicking "+ Add Step" should open the step form **in the same panel area** (details pane or inline)

## 7. Add Step Form — Shared Config with Slash Commands  
- The "Add Step" form and the slash command system should use the **same `app.schema.commands` config** — no duplication
- Some commands have **multiple parameter variants** (e.g. `click` can use `selector`, `text`, `node_id`, or `x`/`y` coords). The form should express this:
  - Show variant groups or a selector for which targeting method to use
  - Only show relevant fields for the selected variant
- The form should be a **click-based UI** (dropdowns, inputs) that mirrors what you can type as a slash command
- When a command has no parameters, don't show any form fields — just the "Add Step" button/action

## 8. Command Variants from DEBUG_API.md
Commands with multiple targeting methods (from the API spec):
- `click`: `selector` | `text` | `node_id` | `x`+`y` coordinates
- `get_node_css_properties`: `node_id` | `selector`
- `get_node_layout`: `node_id` | `selector` | `text`
- `scroll_node_by`: `node_id` | `selector`
- `scroll_node_to`: `node_id` | `selector`
- `get_scrollbar_info`: `node_id` | `selector`

These should be expressed in the schema with variant groups and reflected in both the slash command popup examples and the add-step form.
