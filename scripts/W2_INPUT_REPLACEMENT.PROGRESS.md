# W2 input replacement - progress (branch wt/w2-input-replacement, cut from fix/input-bugs-2026-09-19)

## DONE
- 6dba62aa5 RED tests layout/tests/form_controls_become_widgets.rs (+ all.rs append)

## IN PROGRESS
- design read-through done (icon resolver, style_user_dom_for funnel, dll regenerate_layout
  pre-cascade fingerprint, XML apply_xml_node_attributes, 13 widget APIs)
- impl: layout/src/form_controls.rs

## NEXT (in order)
2. impl: layout/src/form_controls.rs (resolver, ONE mapping table with WAVE2-GLUE(W1) rows,
   FormControlMemory, recorders), LayoutWindow field + style_user_dom_in_scope, VirtualView scope
3. impl: dll regenerate_layout resolves BEFORE the pre-cascade fingerprint
4. impl: XML form attributes in core/src/xml.rs apply_xml_node_attributes; e2e mount path
5. final report scripts/W2_INPUT_REPLACEMENT_2026_09_29.md

## Design decisions (so a resumed session does not re-derive them)
- resolver lives in azul-layout (widgets are there), gated on `widgets`; runs FIRST in
  `LayoutWindow::style_user_dom_*` (before fluent + icons: widgets contain `<icon>`s)
- replacement grafts the raw node's ids/classes/attrs (minus live-state Value/Checked*/Selected),
  inline style (after the widget's), scope-only `Dom.css` rules -> root inline, other rules ->
  scoped sheet, callbacks APPENDED to the widget root, key/marker/context menu/tabindex
- state across rebuilds: `FormControlMemory` (Arc<Mutex>) on LayoutWindow; the widget's typed
  change hook is a resolver RECORDER that writes the user's value keyed by (scope, key|id|path);
  the resolver reads it back only while the app's defaults are unchanged (HTML dirty flag);
  drop-down + radio recorders return RefreshDom (drop-down only shows a value on rebuild; radio
  groups unselect siblings on rebuild). Text additionally survives via the engine overlay.
- opt-out: attribute `data-azul-widget="none"` on the node (works in Rust and XML)
- the dll resolves the root DOM BEFORE `fingerprint_dom`, so the pre-cascade fast path sees the
  same node indices as the retained StyledDom (a raw-DOM fingerprint would transfer callbacks by
  index onto the wrong widget nodes)

## Open questions
- none blocking
