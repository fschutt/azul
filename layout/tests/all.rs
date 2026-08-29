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
//! * `contenteditable_e2e`, `e2e_json`, `text3_suite` — `required-features` is
//!   a per-TARGET switch; a module cannot carry one.
//! * `icu_parity` — CI runs it as
//!   `cargo test --test icu_parity --no-default-features --features icu…`
//!   (`.github/workflows/rust.yml`, job `icu_parity`). Under
//!   `--no-default-features` the other ~116 modules do not compile, so it has
//!   to be a target Cargo can select on its own.
//! * `coretext_autoregression` — `scripts/coretext_regression.sh` invokes it by
//!   name (`--test coretext_autoregression`).
//! * the subdirectory suites (`tests/solver3/`, `tests/managers/`,
//!   `tests/text3/`) plus four root files that were already declared by hand.
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
//! 1. `web_flexbox_simple_ref` set `solver3::SKIP_DISPLAY_LIST` (a global
//!    `AtomicBool`) and never put it back, so every test that ran afterwards
//!    got an empty display list. `xml_dom_embed` measured zero text items.
//! 2. `text3_shaping_cache_identity`'s negative control re-executes
//!    `current_exe()` with `--exact <test name>`. libtest names are now
//!    module-qualified, so the bare name matched nothing, the child ran zero
//!    tests and exited 0, and the control read that as "the defect did not
//!    reproduce" — a gate passing vacuously.
//! 3. `probe_gate` flips the probe recording flag, another process global.
//!    See [`PROBE_LOCK`].
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

#[path = "abs_pos_anomalies.rs"]
mod abs_pos_anomalies;
#[path = "anonymous_nodes.rs"]
mod anonymous_nodes;
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
#[path = "caret_reveal_and_session_identity.rs"]
mod caret_reveal_and_session_identity;
#[path = "caret_scroll_glide.rs"]
mod caret_scroll_glide;
#[path = "caret_tween.rs"]
mod caret_tween;
#[path = "click_into_a_virtual_view_page.rs"]
mod click_into_a_virtual_view_page;
#[path = "cpurender_image_probe.rs"]
mod cpurender_image_probe;
#[path = "cross_block_selection.rs"]
mod cross_block_selection;
#[path = "demo_layout_regressions.rs"]
mod demo_layout_regressions;
#[path = "display_list_ids.rs"]
mod display_list_ids;
#[path = "dl_patch_golden.rs"]
mod dl_patch_golden;
#[path = "document_edit_notify.rs"]
mod document_edit_notify;
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
#[path = "focus_ring_tween.rs"]
mod focus_ring_tween;
#[path = "frame_perf.rs"]
mod frame_perf;
#[path = "gpu_synchronize.rs"]
mod gpu_synchronize;
#[path = "h1_margin_em_resolution.rs"]
mod h1_margin_em_resolution;
#[path = "h1_p_margin_collapse.rs"]
mod h1_p_margin_collapse;
#[path = "hover_manager.rs"]
mod hover_manager;
#[path = "ifc_caching.rs"]
mod ifc_caching;
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
#[path = "keycode_table_manifest_is_exhaustive.rs"]
mod keycode_table_manifest_is_exhaustive;
#[path = "list_marker_counter.rs"]
mod list_marker_counter;
#[path = "loaded_font_introspection.rs"]
mod loaded_font_introspection;
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
#[path = "pagination_dom_breaks.rs"]
mod pagination_dom_breaks;
#[path = "pagination_perf.rs"]
mod pagination_perf;
#[path = "preedit_never_enters_the_text_store.rs"]
mod preedit_never_enters_the_text_store;
#[path = "probe_gate.rs"]
mod probe_gate;
#[path = "regression_font_size_bugs.rs"]
mod regression_font_size_bugs;
#[path = "resize_relayout_bug.rs"]
mod resize_relayout_bug;
#[path = "ribbon_group_overlap.rs"]
mod ribbon_group_overlap;
#[path = "ribbon_tab_whitespace.rs"]
mod ribbon_tab_whitespace;
#[path = "root_box_sizing_regression.rs"]
mod root_box_sizing_regression;
#[path = "session_regression.rs"]
mod session_regression;
#[path = "struct_sizes.rs"]
mod struct_sizes;
#[path = "svg_tessellation.rs"]
mod svg_tessellation;
#[path = "synthetic_events.rs"]
mod synthetic_events;
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
#[path = "scroll_degenerate_ifc.rs"]
mod scroll_degenerate_ifc;
#[path = "radio_group_geometry.rs"]
mod radio_group_geometry;
#[path = "text_edit_seam_regressions.rs"]
mod text_edit_seam_regressions;
#[path = "token_vs_slicer_differential.rs"]
mod token_vs_slicer_differential;
#[path = "unresolved_family_render.rs"]
mod unresolved_family_render;
#[path = "variable_font_disk_path.rs"]
mod variable_font_disk_path;
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
#[path = "window_tests.rs"]
mod window_tests;
#[path = "xml_dom_embed.rs"]
mod xml_dom_embed;
#[path = "xml_no_text_duplication.rs"]
mod xml_no_text_duplication;
#[path = "xml_self_closing.rs"]
mod xml_self_closing;
