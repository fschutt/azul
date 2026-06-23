# Widget gap-list + "add a widget" recipe (build spec)

Research 2026-06-20. Build NEW widgets (each its own file in `layout/src/widgets/`), prioritized
below. Follow the recipe exactly + mirror the named existing widget. One widget = one file = one
commit. Register in `mod.rs` + api.json (via azul-doc autofix, NOT hand-edits).

## Already covered (DO NOT rebuild)
Button, CheckBox, single-line TextInput, NumberInput, DropDown(non-editable select), ProgressBar,
Tabs, TreeView, ListView(columns+sort≈data-grid), Menu/Menubar, native ColorPicker, native
FileDialog/MsgBox (dialog module), Frame(group-box), Titlebar, Image (Dom::create_image), Link
(Button::Link), Ribbon(toolbar-ish), Map, NodeGraph, media(camera/mic/screencap/video), Scroll(CSS overflow).

## Export wins (already in Rust, just add to api.json widgets module — cheap)
- `Label` (Rust-only today), `TabContent` (only TabHeader exported). [Menubar/NodeGraph: keep internal.]

## BUILD QUEUE (prioritized). copy = existing widget whose pattern to reuse.
### Tier 1 (near-universal — build first)
1. Slider/Range — drag thumb on track → numeric value. Moderate. copy: number_input (state) + map.rs (mouse-drag). (mod.rs:190 reserves `slider`)
2. Switch/Toggle — on/off sliding knob. Trivial. copy: check_box (near-verbatim; restyle pill+knob, CSS toggle).
3. Radio group — mutually-exclusive choice. Moderate. copy: check_box visual + drop_down index state (group exclusivity).
4. Divider/Separator — thin rule. Trivial. copy: label (single styled node, no callback).
5. Tooltip — hover popup. Moderate. copy: drop_down popup path (open_menu_for_hit_node/MenuPopupPosition) + hover EventFilter.
6. TextArea (multiline) — multi-line edit. Moderate-complex. copy: text_input extended. (mod.rs:193 reserves `text_edit`)
7. Segmented control / button-group — joined exclusive buttons. Moderate. copy: tabs TabHeader / button.
### Tier 2 (very common)
8. Card — elevated/bordered container. Trivial. copy: frame (drop title, add shadow/radius).
9. Accordion/Expander — collapsible titled sections. Moderate. copy: tree_view (expand state) / tabs.
10. Badge — small count/status pill. Trivial. copy: label.
11. Alert/Banner — colored inline message (+close). Trivial-moderate. copy: frame/label + button(close).
12. Combobox/Autocomplete — editable filtered select. Moderate-complex. copy: drop_down + text_input.
13. Spinner/Activity — indeterminate busy anim. Moderate (anim loop). copy: progressbar (indeterminate mode).
14. Popover — anchored floating panel. Moderate. copy: drop_down / menu popup.
15. Avatar — circular image/initials. Trivial. copy: label/button (round + image/text).
16. Modal/Dialog (in-app custom) — overlay + focus trap. Moderate-complex. copy: frame + overlay layer. (native dialogs already exist; this is the in-app variant)
### Tier 3 (common, heavier)
17. Toast/Snackbar — transient auto-dismiss. Moderate (timer+stack). copy: frame + timed Update.
18. Breadcrumb — path links. Trivial-moderate. copy: horizontal label/button list.
19. Pagination — page-number nav. Moderate. copy: button group / tabs header.
20. Stepper/Wizard — multi-step flow. Moderate-complex. copy: tabs + progressbar.
21. Split pane/Splitter — resizable panes. Moderate-complex (drag divider). copy: frame + map.rs drag.
22. Chip/Tag — compact removable label. Trivial-moderate. copy: button/label.
23. Date picker/Calendar — calendar grid. Complex. copy: new grid + number_input/drop_down; time/ICU modules exist.
24. Time picker — time selection. Complex. copy: number_input spinners.

## THE RECIPE (file:line refs)
- **DOM/style:** widget = struct → `Dom`. Build via `Dom::create_div/create_text/create_node(NodeType::Button)` + `.with_child(ren)`, classes `.with_ids_and_classes(IdOrClassVec::from_const_slice(&[Class(AzString::from_const_str("__azul-native-<name>"))]))`, focus `.with_tab_index(TabIndex::Auto)`. Styles = `CssPropertyWithConditionsVec`: const-static slice (`label.rs:37-53`, `check_box.rs:101-167`, `drop_down.rs:148-211`) OR runtime `from_vec` when style depends on a param (`button.rs:172-295`). Variants: `::simple/::on_hover/::on_active/::on_focus`. Apply `.with_css_props(self.container_style)` (`button.rs:422`).
- **Step1 struct** (`#[repr(C)] #[derive(Debug,Clone,PartialEq)]` + Default): stateless = one struct (`label.rs:16`); interactive-no-state = struct + `Option<…OnClick>` (`button.rs:71`); STATEFUL = 3-type split (`check_box.rs`): `WidgetState{data}` (`:73`), `WidgetStateWrapper{inner,on_event}` (`:64`; two cbs in `number_input.rs:92`), `Widget{state_wrapper,styles}` (`:54`).
- **Step2 callbacks** (after struct, copy `check_box.rs:31-51`): (a) `pub type WidgetOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, WidgetState) -> Update;` (b) `impl_widget_callback!(WidgetOnEvent, OptionWidgetOnEvent, WidgetOnEventCallback, WidgetOnEventCallbackType);` (macro at `mod.rs:18-128`) (c) `azul_core::impl_managed_callback!{ wrapper/info_ty/return_ty/default_ret/invoker_static/invoker_ty/thunk_fn/setter_fn/from_handle_fn, extra_args:[name: Ty] }` (macro at `core/src/host_invoker.rs:293-473`; no-extra form `button.rs:99-109`, extra-arg form `check_box.rs:40-51`).
- **Step3 builders:** `create(...)` fills default styles; per-callback `set_on_event<C:Into<…Callback>>(&mut self, data:RefAny, cb:C)` + chaining `with_on_event(self,…)->Self` (`button.rs:347-364`, `check_box.rs:212-229`); add `swap_with_default(&mut self)->Self` (`button.rs:335`).
- **Step4 `.dom(self)->Dom`:** build tree + styles; wire `.with_callbacks(vec![CoreCallbackData{ event: EventFilter::Hover(HoverEventFilter::MouseUp), callback: CoreCallback{cb: default_on_x as usize, ctx: OptionRefAny::None}, refany: RefAny::new(self.widget_state) }].into())` (`check_box.rs:232-259`; Focus→popup `drop_down.rs:287-301`). Internal `extern "C" fn` handler: downcast RefAny→StateWrapper, mutate, call user cb if present, patch live CSS via `info.set_css_property(node_id, CssProperty::…)` (`check_box.rs:271-315`; value-validate `number_input.rs:292-346`; popup `drop_down.rs:329-374`). Add `impl From<Widget> for Dom`. Containers compose child Doms + content slot (`tabs.rs` TabHeader:1155 + TabContent:1378).
- **Step5 register:** `layout/src/widgets/mod.rs` add `pub mod <name>;` (widgets feature-gated at `lib.rs:118-122`; nothing else in lib.rs). NO manual dll re-exports (generated from api.json). api.json `0.2.0/api/widgets/classes`: add the struct + `*OnEvent`/`*OnEventCallback` + `*State`/`*StateWrapper` + any `*VecSlice` — use **azul-doc autofix** to sync source→api.json (do NOT hand-edit; causes drift). Optionally demo in `examples/c/widgets.c`.

Easiest first (full recipe end-to-end, low risk): #2 Switch, #4 Divider, #8 Card, #10 Badge (near-clones of check_box/label/frame). Then drag-based (#1 Slider, #21 Split) + overlay (#5 Tooltip, #14 Popover, #16 Modal).
