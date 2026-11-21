# A3-window: quadratics and per-frame O(nodes) work in `window.rs`, reconciliation / diff, restyle and list patching

Area agent A3-window of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A3-window`. Base: `audit/base` (`f48bcc7b4`). Line numbers are those of the base commit.

## Claims

First version (from the outline, before the full read); refined as the read goes on.

### Functions A3-window may rewrite or restructure (do not patch inside them; send the finding instead)

`layout/src/window.rs`
- `patch_display_list`, `apply_node_css_change`, `apply_node_style_change`, `changed_declaration_keys`,
  `write_bound_gpu_properties`, `publish_gpu_only_restyle`, `node_states`, `relayout_root_dom_in_place`,
  `apply_image_change`, `apply_background_image_change`
- `regenerate_display_list_for_dom` (the `LayoutWindow` method at 26848)
- `apply_content_change`, `apply_content_change_for_host`
- `unchanged_virtual_views`, `invoke_virtual_view_callback_with_dom`, `invoke_virtual_view_callback_impl`,
  `reclaim_unchanged_child`, `follow_host_in_kept_view`, `repoint_merged_datasets`,
  `reconcile_rematerialized_child`, `process_pending_virtual_view_updates`
- `begin_reconciliation`, `finish_reconciliation`
- `scan_for_virtual_views`, `scan_used_fonts`, `scan_used_images`, `images_in_display_lists`,
  `manager_state_fingerprint`, `compute_scroll_ids`
- `prepare_frame_cpu`, `prepare_frame_gl`, `prepare_frame_content` (only their per-frame scans; not what they call
  in A1's / A4's files)
- the free functions at the end of the file: `sync_css_gpu_values`, `unchanged_cascades`, `changed_styles`,
  `css_diff_between`, `overrides_of`, `types_to_compare`

`dll/src/desktop/shell2/common/event.rs`
- `apply_hover_restyle`, `apply_active_restyle`, `apply_focus_restyle*`, `apply_seat_focus_restyle`, `stage_css_dirty`,
  `gpu_only_restyle_result`, `repaint_restyled_dom`, `regenerate_display_list_for_dom`

`core/src/diff.rs`: all of it. `layout/src/managers/virtual_view.rs`: all of it.

### Not claimed (free for minimal patches by their neighbours)

- `layout_dom_recursive_impl` and `layout_and_generate_display_list*`: read and audited here, but changed by small
  patches only, so A3-solver / A3-core / A6 can patch next to them.
- animation ticks (`tick_animations*`, `park_culled_tracks`, `seed_state_change_transitions`): small patches only.
- everything of A6 (input, hit-test, focus, caret, selection, timers), A2-cpu (4326-4520), A3-core (19632-19900),
  A10 (600-760).

Tests: `layout/src/audit_tests/a3_window.rs`, `dll/src/desktop/audit_tests/a3_window.rs`, and `audit_a3_window_tests`
modules next to private items.

## Findings

(in progress)
