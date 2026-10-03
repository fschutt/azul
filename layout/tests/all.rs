//! ONE integration-test binary for `azul-layout` — every `tests/*.rs` file
//! listed below is a MODULE of this crate, not a crate of its own.
//!
//! # Why
//!
//! Cargo compiles and *links* every auto-discovered `tests/*.rs` as its own
//! binary, each statically pulling in all of `azul-layout` plus its dependency
//! graph. Measured on this tree at `[profile.release]` (`debug = 1`,
//! `strip = false`, both kept deliberately so samply can resolve symbols):
//! **129 test binaries totalling 11.2 GB, averaging 89 MB each.** That was the
//! single largest build cost in the repo — paid on every developer machine and
//! on every CI run of `cargo test -p azul-layout --lib --tests`, and worse on
//! the dev profile, where `debug = 2` applies and no `[profile.dev]` override
//! exists. It filled this machine's disk twice.
//!
//! Folding them links **once**: 14 binaries, 1.16 GB — 89% less linker output.
//! On an 8-core host a cold `cargo test --release -p azul-layout --tests
//! --no-run` drops from 947 s to ~270-315 s, and running the suite from 142 s
//! to ~40-46 s.
//!
//! No coverage moved: every distinct test that ran before still runs. The
//! headline count goes 8407 -> 8380 for two accounted reasons — `common/
//! fakefont.rs` carries 3 `selfcheck` tests and used to be compiled into 11
//! separate modules, so those 3 ran 11 times (-30); and this file's registry
//! guard adds 3.
//!
//! # Adding a test
//!
//! Drop the file in `layout/tests/` and add a `#[path]` line below, in
//! alphabetical order. `autotests = false` in `layout/Cargo.toml` means an
//! unregistered file is **silently not compiled** — it does not fail, it does
//! not warn, it simply never runs. That footgun is closed by
//! `tests/integration_test_registry_is_exhaustive.rs`, which goes red when a
//! `tests/*.rs` file is neither registered here nor declared as its own
//! `[[test]]` in `layout/Cargo.toml`. Do not delete that guard.
//!
//! # Running one file's tests
//!
//! `cargo test --test <file>` no longer addresses a folded test — the target is
//! gone. Filter by module instead, which is the same thing minus a link:
//!
//! ```bash
//! cargo test --release -p azul-layout --test all -- flexbox_integration::
//! ```
//!
//! # What is still its own target
//!
//! `layout/Cargo.toml` keeps a short list of `[[test]]` entries that cannot be
//! modules here:
//!
//! * `contenteditable_e2e`, `e2e_json`, `text3_suite` — `required-features` is a per-TARGET switch;
//!   a module cannot carry one.
//! * `icu_parity` — CI runs it as `cargo test --test icu_parity --no-default-features --features
//!   icu…` (`.github/workflows/rust.yml`, job `icu_parity`). Under `--no-default-features` the
//!   other ~116 modules do not compile, so it has to be a target Cargo can select on its own.
//! * `coretext_autoregression` — the coretext regression suite invokes it by name (`--test
//!   coretext_autoregression`).
//! * the subdirectory suites (`tests/solver3/`, `tests/managers/`, `tests/text3/`) plus four root
//!   files that were already declared by hand.
//!
//! On this host the two name-addressed ones cost ~6 MB each, because both are
//! `#![cfg]`-stripped to nothing off-platform — a rounding error against the
//! ~7.6 GB the fold removes.
//!
//! # Consequence: these tests now share ONE process
//!
//! Each file used to be its own process, so process-global state could not leak
//! between files. It can now, and libtest is multi-threaded by default. Anything
//! touching a `static`, an environment variable, a fixed output path, or its own
//! `current_exe()` has to serialise itself or scope its own state.
//!
//! Folding this tree surfaced three latent instances of exactly that — all of
//! them real defects the old one-process-per-file layout was hiding:
//!
//! 1. `web_flexbox_simple_ref` set `solver3::SKIP_DISPLAY_LIST` (a global `AtomicBool`) and never
//!    put it back, so every test that ran afterwards got an empty display list. `xml_dom_embed`
//!    measured zero text items.
//! 2. `text3_shaping_cache_identity`'s negative control re-executes `current_exe()` with `--exact
//!    <test name>`. libtest names are now module-qualified, so the bare name matched nothing, the
//!    child ran zero tests and exited 0, and the control read that as "the defect did not
//!    reproduce" — a gate passing vacuously.
//! 3. `probe_gate` flips the probe recording flag, another process global. See [`PROBE_LOCK`].
//!
//! Each is fixed and documented at its site.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Serialises the tests that touch `azul_layout::probe`'s PROCESS-GLOBAL
/// recording flag.
///
/// `Probe::set_recording` writes a `static RECORDING: AtomicU8`
/// (`layout/src/probe.rs`); the event buffer it gates is thread-local. While
/// `probe_gate.rs` was its own binary that global had exactly one writer per
/// process — which is precisely what its module docs relied on. In this shared
/// binary, `probe_gate` flipping the flag races `frame_perf` and
/// `pagination_perf`, which drain spans and attribute self-time: they would
/// report a truncated or a phantom profile depending on interleaving, and every
/// *other* test in the binary would start buffering events nobody drains — the
/// unbounded thread-local growth `probe_gate` exists to pin.
///
/// All three take this lock. With the `probe` feature off (the default) the
/// whole probe API is a `const fn` no-op and the lock is free; under
/// `--features probe` it is the thing that keeps them honest.
pub static PROBE_LOCK: Mutex<()> = Mutex::new(());

/// Take [`PROBE_LOCK`], ignoring poisoning.
///
/// A panicking test elsewhere under the lock must not cascade into "this test
/// failed too"; every holder sets the recording flag it wants before reading.
pub fn probe_lock() -> MutexGuard<'static, ()> {
    PROBE_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The deterministic synthetic-font builder in `tests/common/fakefont.rs`,
/// declared ONCE.
///
/// Eleven `text3_*` files used to carry their own
/// `#[path = "common/fakefont.rs"] mod fakefont;`. As eleven separate crates
/// that was eleven independent compilations of one file and nothing could
/// notice; in a single crate `clippy::duplicate_mod` correctly calls it what it
/// is — the same source compiled eleven times into eleven unrelated types. They
/// now `use crate::fakefont` instead.
///
/// Cfg'd to match its users, every one of which is `#![cfg(feature =
/// "text_layout")]`, so a `--no-default-features` build does not compile a
/// module nothing can reach.
#[cfg(feature = "text_layout")]
#[path = "common/fakefont.rs"]
mod fakefont;

// --- the registered integration tests, alphabetically ---

#[path = "a_bold_request_draws_the_bold_instance_of_a_variable_font.rs"]
mod a_bold_request_draws_the_bold_instance_of_a_variable_font;
#[path = "a_caret_counts_bytes_in_its_own_block.rs"]
mod a_caret_counts_bytes_in_its_own_block;
#[path = "a_classic_thumb_stops_above_its_bottom_button.rs"]
mod a_classic_thumb_stops_above_its_bottom_button;
#[path = "a_context_menu_opens_from_a_secondary_press.rs"]
mod a_context_menu_opens_from_a_secondary_press;
#[path = "a_drag_selection_owns_the_scroll_of_its_field.rs"]
mod a_drag_selection_owns_the_scroll_of_its_field;
#[path = "a_press_on_an_overflowing_field_selects.rs"]
mod a_press_on_an_overflowing_field_selects;
#[path = "a_scroll_box_scrolls_to_its_end_padding.rs"]
mod a_scroll_box_scrolls_to_its_end_padding;
#[path = "a_selection_drag_autoscrolls_the_box_its_text_scrolls_in.rs"]
mod a_selection_drag_autoscrolls_the_box_its_text_scrolls_in;
#[path = "a_selection_reveal_shows_its_focus_end.rs"]
mod a_selection_reveal_shows_its_focus_end;
#[path = "a_screen_reader_reads_a_host_with_paragraphs.rs"]
mod a_screen_reader_reads_a_host_with_paragraphs;
#[path = "abs_pos_anomalies.rs"]
mod abs_pos_anomalies;
#[path = "abspos_in_flex_containing_block.rs"]
mod abspos_in_flex_containing_block;
#[path = "accordion_animation.rs"]
mod accordion_animation;
#[path = "anonymous_nodes.rs"]
mod anonymous_nodes;
#[path = "app_caret_moves.rs"]
mod app_caret_moves;
#[path = "app_target.rs"]
mod app_target;
#[path = "azul_widgets_demo_follows_the_theme.rs"]
mod azul_widgets_demo_follows_the_theme;
#[path = "block_edge_keys.rs"]
mod block_edge_keys;
#[path = "block_merge_filter.rs"]
mod block_merge_filter;
#[path = "body_margin_vh.rs"]
mod body_margin_vh;
#[path = "break_token_pages.rs"]
mod break_token_pages;
#[path = "cache_and_dirty_propagation.rs"]
mod cache_and_dirty_propagation;
#[path = "caption_positioning.rs"]
mod caption_positioning;
#[path = "caret_follows_typing.rs"]
mod caret_follows_typing;
#[path = "caret_reveal_across_a_wrap.rs"]
mod caret_reveal_across_a_wrap;
#[path = "caret_reveal_and_session_identity.rs"]
mod caret_reveal_and_session_identity;
#[path = "caret_scroll_glide.rs"]
mod caret_scroll_glide;
#[path = "caret_tween.rs"]
mod caret_tween;
#[path = "carets_set_from_outside.rs"]
mod carets_set_from_outside;
#[path = "clean_pass_keeps_the_layout_cache.rs"]
mod clean_pass_keeps_the_layout_cache;
#[path = "click_into_a_virtual_view_page.rs"]
mod click_into_a_virtual_view_page;
#[path = "cpurender_image_probe.rs"]
mod cpurender_image_probe;
#[path = "cross_block_selection.rs"]
mod cross_block_selection;
#[path = "delete_keyed_to_caret_owner.rs"]
mod delete_keyed_to_caret_owner;
#[path = "damage_raster_reports_what_it_painted.rs"]
mod damage_raster_reports_what_it_painted;
#[path = "demo_layout_regressions.rs"]
mod demo_layout_regressions;
#[path = "display_list_ids.rs"]
mod display_list_ids;
#[path = "dl_patch_golden.rs"]
mod dl_patch_golden;
#[path = "document_edit_notify.rs"]
mod document_edit_notify;
#[path = "document_selection_api.rs"]
mod document_selection_api;
#[path = "drag_image_between_pages_e2e.rs"]
mod drag_image_between_pages_e2e;
#[path = "drag_selection_scroll.rs"]
mod drag_selection_scroll;
#[path = "e2e_pixel_diff.rs"]
mod e2e_pixel_diff;
#[path = "embedded_font_renders.rs"]
mod embedded_font_renders;
#[path = "empty_cells.rs"]
mod empty_cells;
#[path = "flex_intrinsic_text.rs"]
mod flex_intrinsic_text;
#[path = "flex_text_width_bug.rs"]
mod flex_text_width_bug;
#[path = "flexbox_integration.rs"]
mod flexbox_integration;
#[path = "flexbox_stretch_bugs.rs"]
mod flexbox_stretch_bugs;
#[path = "float_and_scrollbar.rs"]
mod float_and_scrollbar;
#[path = "float_integration.rs"]
mod float_integration;
#[path = "focus_manager.rs"]
mod focus_manager;
#[path = "focus_ring_survives_full_relayout.rs"]
mod focus_ring_survives_full_relayout;
#[path = "focus_ring_tween.rs"]
mod focus_ring_tween;
#[path = "frame_perf.rs"]
mod frame_perf;
#[path = "global_hotkeys.rs"]
mod global_hotkeys;
#[path = "gpu_synchronize.rs"]
mod gpu_synchronize;
#[path = "h1_margin_em_resolution.rs"]
mod h1_margin_em_resolution;
#[path = "h1_p_margin_collapse.rs"]
mod h1_p_margin_collapse;
#[path = "hover_manager.rs"]
mod hover_manager;
#[path = "icon_pipeline.rs"]
mod icon_pipeline;
#[path = "ifc_caching.rs"]
mod ifc_caching;
#[path = "ime_geometry_follows_the_fields_scroll.rs"]
mod ime_geometry_follows_the_fields_scroll;
#[path = "injected_chrome_takes_its_own_space.rs"]
mod injected_chrome_takes_its_own_space;
#[path = "inline_atomic_after_block.rs"]
mod inline_atomic_after_block;

#[path = "atomic_inline_paint_once.rs"]
mod atomic_inline_paint_once;

#[path = "a11y_consumer_contract.rs"]
mod a11y_consumer_contract;
#[path = "image_child_paint.rs"]
mod image_child_paint;
#[path = "image_flex_grow.rs"]
mod image_flex_grow;
#[path = "incremental_rendering.rs"]
mod incremental_rendering;
#[path = "inline_block_text.rs"]
mod inline_block_text;
#[path = "inline_gradient_border.rs"]
mod inline_gradient_border;
#[path = "integration_test_registry_is_exhaustive.rs"]
mod integration_test_registry_is_exhaustive;
#[path = "keyboard_selection_is_painted.rs"]
mod keyboard_selection_is_painted;
#[path = "keycode_table_manifest_is_exhaustive.rs"]
mod keycode_table_manifest_is_exhaustive;
#[path = "list_item_editing.rs"]
mod list_item_editing;
#[path = "list_marker_counter.rs"]
mod list_marker_counter;
#[path = "loaded_font_introspection.rs"]
mod loaded_font_introspection;
#[path = "materialized_inline_layout.rs"]
mod materialized_inline_layout;
#[path = "map_widget_fill.rs"]
mod map_widget_fill;
#[path = "margin_collapse_integration.rs"]
mod margin_collapse_integration;
#[path = "margin_collapsing.rs"]
mod margin_collapsing;
#[path = "margin_collapsing_bug.rs"]
mod margin_collapsing_bug;
#[path = "margin_escape_regression.rs"]
mod margin_escape_regression;
#[path = "media_restyle_cost.rs"]
mod media_restyle_cost;
#[path = "menubar_item_clip.rs"]
mod menubar_item_clip;
#[path = "mock_font_metrics.rs"]
mod mock_font_metrics;
#[path = "multi_range_selection.rs"]
mod multi_range_selection;
#[path = "native_notifications.rs"]
mod native_notifications;
#[path = "drag_into_an_empty_line.rs"]
mod drag_into_an_empty_line;
#[path = "an_arrow_collapses_a_document_selection_like_a_click.rs"]
mod an_arrow_collapses_a_document_selection_like_a_click;
#[path = "an_svg_without_a_viewbox.rs"]
mod an_svg_without_a_viewbox;
#[path = "a_mask_clip_on_a_half_pixel.rs"]
mod a_mask_clip_on_a_half_pixel;
#[path = "pagination_dom_breaks.rs"]
mod pagination_dom_breaks;
#[path = "pagination_fits_its_card.rs"]
mod pagination_fits_its_card;
#[path = "shaping_cache_keeps_the_run_it_is_hit_from.rs"]
mod shaping_cache_keeps_the_run_it_is_hit_from;
#[path = "caret_in_an_inline_editing_host.rs"]
mod caret_in_an_inline_editing_host;
#[path = "inline_media_follows_source_order.rs"]
mod inline_media_follows_source_order;
#[path = "fixed_size_widgets_sit_at_the_start.rs"]
mod fixed_size_widgets_sit_at_the_start;
#[path = "patched_opacity_reaches_the_paint.rs"]
mod patched_opacity_reaches_the_paint;
#[path = "pagination_perf.rs"]
mod pagination_perf;
#[path = "preedit_never_enters_the_text_store.rs"]
mod preedit_never_enters_the_text_store;
#[path = "probe_gate.rs"]
mod probe_gate;
#[path = "radio_group_geometry.rs"]
mod radio_group_geometry;
#[path = "regression_font_size_bugs.rs"]
mod regression_font_size_bugs;
#[path = "resize_relayout_bug.rs"]
mod resize_relayout_bug;
#[path = "restored_caret_lands_in_its_text.rs"]
mod restored_caret_lands_in_its_text;
#[path = "ribbon_group_overlap.rs"]
mod ribbon_group_overlap;
#[path = "ribbon_tab_whitespace.rs"]
mod ribbon_tab_whitespace;
#[path = "root_box_sizing_regression.rs"]
mod root_box_sizing_regression;
#[path = "run_remap.rs"]
mod run_remap;
#[path = "safe_area_inset.rs"]
mod safe_area_inset;
#[path = "scroll_box_reserves_its_gutter.rs"]
mod scroll_box_reserves_its_gutter;
#[path = "scroll_chain.rs"]
mod scroll_chain;
#[path = "scroll_degenerate_ifc.rs"]
mod scroll_degenerate_ifc;
#[path = "scroll_id_identity.rs"]
mod scroll_id_identity;
#[path = "scroll_shift_ghost.rs"]
mod scroll_shift_ghost;
#[path = "scrollbar_fade_during_drag.rs"]
mod scrollbar_fade_during_drag;
#[path = "scrollbar_presence.rs"]
mod scrollbar_presence;
#[path = "seat_text_session.rs"]
mod seat_text_session;
#[path = "select_all_covers_its_host.rs"]
mod select_all_covers_its_host;
#[path = "selection_handles.rs"]
mod selection_handles;
#[path = "selection_skips_unselectable_text.rs"]
mod selection_skips_unselectable_text;
#[path = "session_regression.rs"]
mod session_regression;
#[path = "shift_arrows_extend_a_document_selection.rs"]
mod shift_arrows_extend_a_document_selection;
#[path = "single_block_copy.rs"]
mod single_block_copy;
#[path = "spatial_navigation.rs"]
mod spatial_navigation;
#[path = "stale_document_selection.rs"]
mod stale_document_selection;
#[path = "statusbar_live_label.rs"]
mod statusbar_live_label;
#[path = "struct_sizes.rs"]
mod struct_sizes;
#[path = "static_opacity_paints.rs"]
mod static_opacity_paints;
#[path = "subtree_relayout.rs"]
mod subtree_relayout;
#[path = "svg_paint.rs"]
mod svg_paint;
#[path = "svg_tessellation.rs"]
mod svg_tessellation;
#[path = "switch_animation.rs"]
mod switch_animation;
#[path = "synthetic_events.rs"]
mod synthetic_events;
#[path = "system_colour_keywords.rs"]
mod system_colour_keywords;
#[path = "system_colours_in_every_colour_property.rs"]
mod system_colours_in_every_colour_property;
#[path = "table_cell_width.rs"]
mod table_cell_width;
#[path = "table_cell_width_diag.rs"]
mod table_cell_width_diag;
#[path = "table_layout.rs"]
mod table_layout;
#[path = "table_width_and_alignment.rs"]
mod table_width_and_alignment;
#[path = "taffy_stretch_test.rs"]
mod taffy_stretch_test;
#[path = "test_bytecode_decode.rs"]
mod test_bytecode_decode;
#[path = "test_coretext_compare.rs"]
mod test_coretext_compare;
#[path = "test_font_family_parsing.rs"]
mod test_font_family_parsing;
#[path = "test_glyph_cache_shaping.rs"]
mod test_glyph_cache_shaping;
#[path = "test_html_body_selector.rs"]
mod test_html_body_selector;
#[path = "test_ligature_shaping.rs"]
mod test_ligature_shaping;
#[path = "test_list_counters.rs"]
mod test_list_counters;
#[path = "test_scrollbar_detection.rs"]
mod test_scrollbar_detection;
#[path = "test_style_tag_parsing.rs"]
mod test_style_tag_parsing;
#[path = "test_text_layout.rs"]
mod test_text_layout;
#[path = "text3_baseline_exact.rs"]
mod text3_baseline_exact;
#[path = "text3_brutal_selection.rs"]
mod text3_brutal_selection;
#[path = "text3_brutal_shaping.rs"]
mod text3_brutal_shaping;
#[path = "text3_brutal_solver3.rs"]
mod text3_brutal_solver3;
#[path = "text3_cluster_source_roundtrip.rs"]
mod text3_cluster_source_roundtrip;
#[path = "text3_cursor_exact.rs"]
mod text3_cursor_exact;
#[path = "text3_dense_equivalence.rs"]
mod text3_dense_equivalence;
#[path = "text3_dropcap_baseline_visual.rs"]
mod text3_dropcap_baseline_visual;
#[path = "text3_regression_bidi.rs"]
mod text3_regression_bidi;
#[path = "text3_regression_breaking.rs"]
mod text3_regression_breaking;
#[path = "text3_regression_metrics.rs"]
mod text3_regression_metrics;
#[path = "text3_regression_selection_edit.rs"]
mod text3_regression_selection_edit;
#[path = "text3_regression_solver3.rs"]
mod text3_regression_solver3;
#[path = "text3_regression_whitespace.rs"]
mod text3_regression_whitespace;
#[path = "text3_selection_exact.rs"]
mod text3_selection_exact;
#[path = "text3_shaping_cache_identity.rs"]
mod text3_shaping_cache_identity;
#[path = "text3_shaping_exact.rs"]
mod text3_shaping_exact;
#[path = "text3_visual.rs"]
mod text3_visual;
#[path = "text_ack_survives_relayout.rs"]
mod text_ack_survives_relayout;
#[path = "text_beside_a_block_is_selectable.rs"]
mod text_beside_a_block_is_selectable;
#[path = "text_block_resolver.rs"]
mod text_block_resolver;
#[path = "text_edit_seam_regressions.rs"]
mod text_edit_seam_regressions;
#[path = "textarea_enter_repaint.rs"]
mod textarea_enter_repaint;
#[path = "textinput_first_draw_and_focus.rs"]
mod textinput_first_draw_and_focus;
#[path = "textinput_resize_selection.rs"]
mod textinput_resize_selection;
#[path = "textinput_seed_style.rs"]
mod textinput_seed_style;
#[path = "the_macos_titlebar_lines_up_with_its_traffic_lights.rs"]
mod the_macos_titlebar_lines_up_with_its_traffic_lights;
#[path = "theme_conditional_stylesheet.rs"]
mod theme_conditional_stylesheet;
#[path = "token_vs_slicer_differential.rs"]
mod token_vs_slicer_differential;
#[path = "tray_events.rs"]
mod tray_events;
#[path = "typed_script_font_fallback.rs"]
mod typed_script_font_fallback;
#[path = "typing_beside_a_block.rs"]
mod typing_beside_a_block;
#[path = "typing_into_a_formatted_paragraph.rs"]
mod typing_into_a_formatted_paragraph;
#[path = "typing_past_the_right_edge_reveals_the_newest_character.rs"]
mod typing_past_the_right_edge_reveals_the_newest_character;
#[path = "unresolved_family_render.rs"]
mod unresolved_family_render;
#[path = "variable_font_disk_path.rs"]
mod variable_font_disk_path;
#[path = "viewport_scroll_frame.rs"]
mod viewport_scroll_frame;
#[path = "viewport_scrollbar.rs"]
mod viewport_scrollbar;
#[path = "viewport_scrolls_its_overflow.rs"]
mod viewport_scrolls_its_overflow;
#[path = "virtual_view_natural_size.rs"]
mod virtual_view_natural_size;
#[path = "virtualized_view_manager.rs"]
mod virtualized_view_manager;
#[path = "visibility_collapse.rs"]
mod visibility_collapse;
#[path = "vview_contenteditable_e2e.rs"]
mod vview_contenteditable_e2e;
#[path = "web_events_repro.rs"]
mod web_events_repro;
#[path = "web_flexbox_simple_ref.rs"]
mod web_flexbox_simple_ref;
#[path = "web_lift_nested_text_repro.rs"]
mod web_lift_nested_text_repro;
#[path = "whitespace_processing.rs"]
mod whitespace_processing;
#[path = "widget_lint_manifest_is_exhaustive.rs"]
mod widget_lint_manifest_is_exhaustive;
#[path = "window_control_click.rs"]
mod window_control_click;
#[path = "window_tests.rs"]
mod window_tests;
#[path = "xml_dom_embed.rs"]
mod xml_dom_embed;
#[path = "xml_no_text_duplication.rs"]
mod xml_no_text_duplication;
#[path = "xml_self_closing.rs"]
mod xml_self_closing;
#[path = "zero_width_selection.rs"]
mod zero_width_selection;
#[path = "form_controls_become_widgets.rs"]
mod form_controls_become_widgets;
#[path = "flat_and_flora_widgets_follow_the_light_and_dark_theme.rs"]
mod flat_and_flora_widgets_follow_the_light_and_dark_theme;
#[path = "app_color_scheme_override.rs"]
mod app_color_scheme_override;
#[path = "app_theme_override.rs"]
mod app_theme_override;
#[path = "widgets_follow_the_app_theme.rs"]
mod widgets_follow_the_app_theme;
#[path = "a_theme_chain_ranks_its_blocks.rs"]
mod a_theme_chain_ranks_its_blocks;
#[path = "rice_styles_the_window.rs"]
mod rice_styles_the_window;
#[path = "a_tinted_raster_icon_is_tinted_inside_its_own_alpha.rs"]
mod a_tinted_raster_icon_is_tinted_inside_its_own_alpha;
#[path = "an_svg_icon_follows_the_colour_of_its_node.rs"]
mod an_svg_icon_follows_the_colour_of_its_node;
#[path = "user_icon_rules_follow_the_theme_chain.rs"]
mod user_icon_rules_follow_the_theme_chain;
#[path = "a_scroll_box_keeps_its_blit_on_a_scrolled_page.rs"]
mod a_scroll_box_keeps_its_blit_on_a_scrolled_page;
#[path = "a_layout_blit_repaints_the_scrollbar_it_dragged.rs"]
mod a_layout_blit_repaints_the_scrollbar_it_dragged;
#[path = "a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted.rs"]
mod a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted;
#[path = "a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it.rs"]
mod a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it;
#[path = "a_drag_autoscrolls_the_box_its_containing_block_scrolls_in.rs"]
mod a_drag_autoscrolls_the_box_its_containing_block_scrolls_in;
#[path = "a_thin_scrollbar_is_pressed_where_it_is_painted.rs"]
mod a_thin_scrollbar_is_pressed_where_it_is_painted;
#[path = "backdrop_follows_window_activation.rs"]
mod backdrop_follows_window_activation;
#[path = "a_box_shadow_paints_once.rs"]
mod a_box_shadow_paints_once;
#[path = "a_replaced_inline_style_follows_the_mode.rs"]
mod a_replaced_inline_style_follows_the_mode;
#[path = "a_clicked_control_takes_the_new_mode_after_a_scheme_switch.rs"]
mod a_clicked_control_takes_the_new_mode_after_a_scheme_switch;
#[path = "app_set_text_beats_typing.rs"]
mod app_set_text_beats_typing;
#[path = "an_app_names_itself_with_app_id.rs"]
mod an_app_names_itself_with_app_id;
#[path = "a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules.rs"]
mod a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules;
#[path = "ctrl_d_searches_for_the_whole_word.rs"]
mod ctrl_d_searches_for_the_whole_word;
#[path = "shift_down_off_a_paragraph_keeps_the_column.rs"]
mod shift_down_off_a_paragraph_keeps_the_column;
#[path = "text_after_a_line_break_is_edited_at_its_caret.rs"]
mod text_after_a_line_break_is_edited_at_its_caret;
#[path = "typing_after_collapsed_spaces_lands_at_the_caret.rs"]
mod typing_after_collapsed_spaces_lands_at_the_caret;
#[path = "a_list_item_caret_moves_with_the_apps_text.rs"]
mod a_list_item_caret_moves_with_the_apps_text;
#[path = "a_screen_reader_reads_inline_text_where_it_stands.rs"]
mod a_screen_reader_reads_inline_text_where_it_stands;
#[path = "a_reveal_scrolls_only_the_boxes_that_move_its_target.rs"]
mod a_reveal_scrolls_only_the_boxes_that_move_its_target;
#[path = "an_arrow_reads_the_action_of_the_scroll_box_it_is_painted_in.rs"]
mod an_arrow_reads_the_action_of_the_scroll_box_it_is_painted_in;
#[path = "a_spatial_navigation_container_is_a_scroll_box_its_node_is_painted_in.rs"]
mod a_spatial_navigation_container_is_a_scroll_box_its_node_is_painted_in;
#[path = "the_ime_caret_rect_is_where_the_raster_paints_the_caret.rs"]
mod the_ime_caret_rect_is_where_the_raster_paints_the_caret;
#[path = "a_layout_blit_repaints_what_is_painted_over_its_mover.rs"]
mod a_layout_blit_repaints_what_is_painted_over_its_mover;
#[path = "a_scrollbar_in_a_transformed_virtual_view_is_pressed_where_it_is_painted.rs"]
mod a_scrollbar_in_a_transformed_virtual_view_is_pressed_where_it_is_painted;
#[path = "a_nodes_own_hover_block_applies_only_when_hovered.rs"]
mod a_nodes_own_hover_block_applies_only_when_hovered;
#[path = "a_node_restyled_to_other_variables_resolves_them.rs"]
mod a_node_restyled_to_other_variables_resolves_them;
#[path = "a_full_width_rule_in_a_spanning_table_cell_renders.rs"]
mod a_full_width_rule_in_a_spanning_table_cell_renders;
#[path = "a_linear_gradient_puts_its_colours_where_css_says.rs"]
mod a_linear_gradient_puts_its_colours_where_css_says;
#[path = "a_scroll_area_under_a_fixed_header_reaches_its_whole_content.rs"]
mod a_scroll_area_under_a_fixed_header_reaches_its_whole_content;
#[path = "a_receipts_price_column_sits_beside_its_labels_under_a_full_width_rule.rs"]
mod a_receipts_price_column_sits_beside_its_labels_under_a_full_width_rule;
#[path = "a_heading_and_paragraph_in_an_indented_table_cell_paint_their_text.rs"]
mod a_heading_and_paragraph_in_an_indented_table_cell_paint_their_text;
#[path = "quote_bars_from_one_gradient_paint_each_colour_at_its_length.rs"]
mod quote_bars_from_one_gradient_paint_each_colour_at_its_length;
#[path = "a_window_paces_at_its_monitors_refresh_rate.rs"]
mod a_window_paces_at_its_monitors_refresh_rate;
#[path = "the_resize_fast_path_paints_what_a_relayout_paints.rs"]
mod the_resize_fast_path_paints_what_a_relayout_paints;
#[path = "a_link_in_mail_markup_keeps_where_it_points.rs"]
mod a_link_in_mail_markup_keeps_where_it_points;
#[path = "the_named_entities_mail_uses_decode_to_their_characters.rs"]
mod the_named_entities_mail_uses_decode_to_their_characters;
#[path = "a_stylesheet_wrapped_in_comment_markers_keeps_its_rules.rs"]
mod a_stylesheet_wrapped_in_comment_markers_keeps_its_rules;
#[path = "mail_markup_gets_the_html_rendering_defaults.rs"]
mod mail_markup_gets_the_html_rendering_defaults;
#[path = "a_block_holding_only_a_line_break_is_one_line_tall.rs"]
mod a_block_holding_only_a_line_break_is_one_line_tall;
#[path = "a_list_marker_is_painted_inside_its_text_clip.rs"]
mod a_list_marker_is_painted_inside_its_text_clip;
#[path = "an_underline_covers_the_last_letter_of_its_run.rs"]
mod an_underline_covers_the_last_letter_of_its_run;
#[path = "a_click_on_a_link_inside_a_paragraph_reaches_the_link.rs"]
mod a_click_on_a_link_inside_a_paragraph_reaches_the_link;
#[path = "a_components_declared_arguments_reach_its_render_fn.rs"]
mod a_components_declared_arguments_reach_its_render_fn;
#[path = "a_table_cell_with_loose_text_and_a_block_paints_both.rs"]
mod a_table_cell_with_loose_text_and_a_block_paints_both;
#[path = "a_narrow_table_wraps_its_cells_to_fit.rs"]
mod a_narrow_table_wraps_its_cells_to_fit;
#[path = "common/editing_harness.rs"]
mod editing_harness;
#[path = "a_format_toggle_at_a_caret_styles_what_is_typed_next.rs"]
mod a_format_toggle_at_a_caret_styles_what_is_typed_next;
#[path = "a_plain_arrow_crosses_the_blocks_of_its_editing_host.rs"]
mod a_plain_arrow_crosses_the_blocks_of_its_editing_host;
#[path = "a_delete_across_blocks_keeps_the_surviving_runs.rs"]
mod a_delete_across_blocks_keeps_the_surviving_runs;
#[path = "a_rich_paste_inserts_formatting_and_blocks.rs"]
mod a_rich_paste_inserts_formatting_and_blocks;
#[path = "a_reset_editor_takes_the_apps_new_content.rs"]
mod a_reset_editor_takes_the_apps_new_content;
#[path = "enter_in_a_nested_quote_splits_the_paragraph_not_the_quote.rs"]
mod enter_in_a_nested_quote_splits_the_paragraph_not_the_quote;
#[path = "a_scrolled_virtual_view_is_repainted_where_its_content_moved.rs"]
mod a_scrolled_virtual_view_is_repainted_where_its_content_moved;
#[path = "a_detected_pinch_has_no_padding.rs"]
mod a_detected_pinch_has_no_padding;
#[path = "flex_items_keep_the_size_their_container_gave_them.rs"]
mod flex_items_keep_the_size_their_container_gave_them;
#[path = "common/table_markup.rs"]
mod table_markup;
#[path = "a_table_is_as_wide_as_its_content_and_container_allow.rs"]
mod a_table_is_as_wide_as_its_content_and_container_allow;
#[path = "row_groups_are_boxes_stacked_in_order.rs"]
mod row_groups_are_boxes_stacked_in_order;
#[path = "presentational_table_attributes_style_the_table.rs"]
mod presentational_table_attributes_style_the_table;
#[path = "percentage_and_fixed_columns_share_the_table_like_browsers.rs"]
mod percentage_and_fixed_columns_share_the_table_like_browsers;
#[path = "a_nested_table_widens_the_cell_that_holds_it.rs"]
mod a_nested_table_widens_the_cell_that_holds_it;
#[path = "a_render_image_callback_with_unchanged_inputs_is_not_invoked_again.rs"]
mod a_render_image_callback_with_unchanged_inputs_is_not_invoked_again;
#[path = "real_mail_html_parses_like_a_browser.rs"]
mod real_mail_html_parses_like_a_browser;
#[path = "the_two_xml_loaders_build_one_tree.rs"]
mod the_two_xml_loaders_build_one_tree;
#[path = "html_pasted_from_word_and_browsers_keeps_its_formatting.rs"]
mod html_pasted_from_word_and_browsers_keeps_its_formatting;
#[path = "builtin_html_elements_take_their_presentational_arguments.rs"]
mod builtin_html_elements_take_their_presentational_arguments;
#[path = "the_list_style_shorthand_sets_the_marker_type_and_position.rs"]
mod the_list_style_shorthand_sets_the_marker_type_and_position;
#[path = "a_cells_specified_width_is_its_columns_width.rs"]
mod a_cells_specified_width_is_its_columns_width;
#[path = "a_collapsed_table_shares_each_border_between_its_cells.rs"]
mod a_collapsed_table_shares_each_border_between_its_cells;
#[path = "a_separated_table_spaces_its_cells_and_paints_its_own_border.rs"]
mod a_separated_table_spaces_its_cells_and_paints_its_own_border;
#[path = "a_rows_height_is_its_tallest_cell_or_its_own_height.rs"]
mod a_rows_height_is_its_tallest_cell_or_its_own_height;
#[path = "a_fixed_table_takes_its_column_widths_from_its_first_row.rs"]
mod a_fixed_table_takes_its_column_widths_from_its_first_row;
#[path = "a_spanning_cells_width_is_spread_over_the_columns_it_spans.rs"]
mod a_spanning_cells_width_is_spread_over_the_columns_it_spans;
#[path = "an_acked_split_of_a_list_item_resumes_past_the_new_items_marker.rs"]
mod an_acked_split_of_a_list_item_resumes_past_the_new_items_marker;
#[path = "a_chat_field_keeps_its_width_while_text_is_typed.rs"]
mod a_chat_field_keeps_its_width_while_text_is_typed;
#[path = "a_partial_image_change_leaves_its_rect_for_the_renderer.rs"]
mod a_partial_image_change_leaves_its_rect_for_the_renderer;
#[path = "an_img_from_markup_shows_the_image_the_app_cached_under_its_src.rs"]
mod an_img_from_markup_shows_the_image_the_app_cached_under_its_src;
#[path = "content_clipped_by_an_overflow_hidden_box_adds_no_pages.rs"]
mod content_clipped_by_an_overflow_hidden_box_adds_no_pages;
#[path = "a_multicol_block_flows_its_children_through_its_columns.rs"]
mod a_multicol_block_flows_its_children_through_its_columns;
#[path = "a_percent_wide_inline_image_takes_its_share_of_the_line_box_container.rs"]
mod a_percent_wide_inline_image_takes_its_share_of_the_line_box_container;
#[path = "the_first_line_of_a_paragraph_starts_text_indent_further_in.rs"]
mod the_first_line_of_a_paragraph_starts_text_indent_further_in;
#[path = "a_line_height_in_points_sets_the_line_pitch.rs"]
mod a_line_height_in_points_sets_the_line_pitch;
#[path = "an_absolute_line_height_is_the_exact_line_pitch.rs"]
mod an_absolute_line_height_is_the_exact_line_pitch;
#[path = "a_line_height_in_em_or_percent_inherits_as_a_length.rs"]
mod a_line_height_in_em_or_percent_inherits_as_a_length;
#[path = "an_inline_block_inside_a_span_is_sized_by_its_own_css.rs"]
mod an_inline_block_inside_a_span_is_sized_by_its_own_css;
#[path = "a_text_edit_reports_the_formats_of_its_text.rs"]
mod a_text_edit_reports_the_formats_of_its_text;
#[path = "an_auto_height_block_stops_growing_at_its_max_height.rs"]
mod an_auto_height_block_stops_growing_at_its_max_height;
#[path = "a_gmail_quote_is_indented_by_its_ex_margin.rs"]
mod a_gmail_quote_is_indented_by_its_ex_margin;
#[path = "an_unresolved_img_from_markup_takes_no_space.rs"]
mod an_unresolved_img_from_markup_takes_no_space;
#[path = "a_blank_line_is_as_tall_as_a_line_of_text.rs"]
mod a_blank_line_is_as_tall_as_a_line_of_text;
#[path = "a_rich_text_editor_keeps_one_model_and_one_history.rs"]
mod a_rich_text_editor_keeps_one_model_and_one_history;
#[path = "text_beside_an_italic_or_bold_box_keeps_a_font.rs"]
mod text_beside_an_italic_or_bold_box_keeps_a_font;
#[path = "a_rows_stray_child_sits_in_an_anonymous_cell.rs"]
mod a_rows_stray_child_sits_in_an_anonymous_cell;
#[path = "an_inline_block_contributes_its_clamped_padded_width.rs"]
mod an_inline_block_contributes_its_clamped_padded_width;
#[path = "a_cells_vertical_align_counts_its_last_childs_bottom_margin.rs"]
mod a_cells_vertical_align_counts_its_last_childs_bottom_margin;
#[path = "a_line_break_ends_a_line_in_the_max_content.rs"]
mod a_line_break_ends_a_line_in_the_max_content;
#[path = "an_inline_block_in_a_cell_sits_where_its_line_puts_it.rs"]
mod an_inline_block_in_a_cell_sits_where_its_line_puts_it;
#[path = "a_space_between_a_tables_inline_children_is_kept.rs"]
mod a_space_between_a_tables_inline_children_is_kept;
#[path = "an_inline_tables_baseline_is_its_first_rows.rs"]
mod an_inline_tables_baseline_is_its_first_rows;
#[path = "a_captions_own_caption_side_places_it.rs"]
mod a_captions_own_caption_side_places_it;
#[path = "a_spanning_cells_percentage_is_shared_by_its_columns.rs"]
mod a_spanning_cells_percentage_is_shared_by_its_columns;
#[path = "a_right_to_left_tables_columns_run_from_the_right.rs"]
mod a_right_to_left_tables_columns_run_from_the_right;
#[path = "an_apps_shell_body_fills_its_window.rs"]
mod an_apps_shell_body_fills_its_window;
#[path = "a_normal_line_is_as_tall_as_chromes.rs"]
mod a_normal_line_is_as_tall_as_chromes;
#[path = "a_cell_of_only_inline_boxes_aligns_them_like_text.rs"]
mod a_cell_of_only_inline_boxes_aligns_them_like_text;
#[path = "a_right_to_left_collapsed_border_stays_on_its_side.rs"]
mod a_right_to_left_collapsed_border_stays_on_its_side;
#[path = "an_atomic_inline_inside_a_span_keeps_its_box.rs"]
mod an_atomic_inline_inside_a_span_keeps_its_box;
#[path = "a_percentage_height_in_an_auto_height_block_is_auto.rs"]
mod a_percentage_height_in_an_auto_height_block_is_auto;
#[path = "an_overflowing_line_overflows_past_its_end_edge.rs"]
mod an_overflowing_line_overflows_past_its_end_edge;
#[path = "a_virtual_views_child_dom_state_goes_with_its_host.rs"]
mod a_virtual_views_child_dom_state_goes_with_its_host;
#[path = "a_transformed_box_is_hit_where_it_is_painted.rs"]
mod a_transformed_box_is_hit_where_it_is_painted;
#[path = "a_rebuild_transitions_only_what_its_window_sees_change.rs"]
mod a_rebuild_transitions_only_what_its_window_sees_change;
#[path = "a_text_rasterises_into_a_raw_image.rs"]
mod a_text_rasterises_into_a_raw_image;
#[path = "an_image_patched_in_place_survives_a_cached_relayout.rs"]
mod an_image_patched_in_place_survives_a_cached_relayout;
#[path = "the_undo_keys_are_a_default_action_an_editor_can_veto.rs"]
mod the_undo_keys_are_a_default_action_an_editor_can_veto;
#[path = "typing_stays_with_its_field_when_another_page_replaces_it.rs"]
mod typing_stays_with_its_field_when_another_page_replaces_it;
#[path = "text_inside_an_opacity_group_keeps_its_colour.rs"]
mod text_inside_an_opacity_group_keeps_its_colour;
#[path = "a_text_field_takes_the_font_size_its_app_gives_it.rs"]
mod a_text_field_takes_the_font_size_its_app_gives_it;
#[path = "a_stretched_flex_container_keeps_its_min_height.rs"]
mod a_stretched_flex_container_keeps_its_min_height;
#[path = "a_rich_text_editor_sets_its_line_height_and_scales_its_indents_with_its_text.rs"]
mod a_rich_text_editor_sets_its_line_height_and_scales_its_indents_with_its_text;
