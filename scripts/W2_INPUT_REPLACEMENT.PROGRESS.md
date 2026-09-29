# W2 input replacement - progress (branch wt/w2-input-replacement, cut from fix/input-bugs-2026-09-19)

## DONE
- 6dba62aa5 RED tests layout/tests/form_controls_become_widgets.rs (+ all.rs append)
- 59fd16e7e feat: layout/src/form_controls.rs (resolver, ONE table with WAVE2-GLUE(W1) rows,
  FormControlMemory + recorders), LayoutWindow.form_control_memory (4 field sites),
  resolve_form_controls / style_user_dom_in_scope / FORM_SCOPE_ROOT|MEASURE /
  form_scope_of_virtual_view, VirtualView + measure scopes, dll regenerate_layout resolves before
  the pre-cascade fingerprint, E2E XML mount (parse_xml_to_styled_dom_resolving_icons)

## IN PROGRESS
- core/src/xml.rs: form_control_attributes() in apply_xml_node_attributes (written, uncommitted)

## NEXT (in order)
1. commit the XML attribute translation
2. final report scripts/W2_INPUT_REPLACEMENT_2026_09_29.md

## Design decisions (so a resumed session does not re-derive them)
- resolver lives in azul-layout (widgets are there), gated on `widgets`; runs FIRST in
  `LayoutWindow::style_user_dom_*` (before fluent + icons: widgets contain `<icon>`s)
- replacement grafts the raw node's ids/classes/attrs (minus live-state Value/Checked*/Selected/
  Placeholder), inline style (after the widget's), scope-only `Dom.css` rules -> root inline,
  other rules -> scoped sheet, callbacks APPENDED to the widget root, key/marker/menus/tabindex
- state across rebuilds: `FormControlMemory` (Arc<Mutex>) on LayoutWindow; the widget's typed
  change hook is a resolver RECORDER that writes the user's value keyed by (scope, key|id|path);
  the resolver reads it back only while the app's defaults are unchanged (HTML dirty flag);
  drop-down + radio recorders return RefreshDom. Text additionally survives via the overlay.
- measured DOMs use FORM_SCOPE_MEASURE so a throwaway DOM cannot evict a real control's value
- opt-out: attribute `data-azul-widget="none"` on the node (works in Rust and XML)
- the dll resolves the root DOM BEFORE `fingerprint_dom`

## Open questions
- none blocking
