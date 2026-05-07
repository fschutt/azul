//! Compile-time coverage gate for the public `Dom` constructor surface.
//!
//! Each function listed in `/tmp/azul-rename-plan.md` is referenced below.
//! The test never executes its assertions; the bodies live behind a
//! never-taken `if false { ... }` branch. If any constructor is removed or
//! renamed in a future refactor, this test stops compiling and the change
//! is caught at CI time.

#![allow(unused_imports, dead_code, unused_variables, clippy::all)]

use azul_core::a11y::SmallAriaInfo;
use azul_core::callbacks::VirtualViewCallback;
use azul_core::dom::{Dom, NodeData, NodeType};
use azul_core::refany::RefAny;
use azul_core::resources::ImageRef;
use azul_css::{AzString, OptionString};

// ---------------------------------------------------------------------------
// Empty (no-arg) constructors for every builtin element.
// ---------------------------------------------------------------------------

fn _coverage_no_arg_document() {
    let _ = Dom::create_html();
    let _ = Dom::create_head();
    let _ = Dom::create_body();
    let _ = Dom::create_div();
}

fn _coverage_no_arg_sectioning() {
    let _ = Dom::create_header();
    let _ = Dom::create_footer();
    let _ = Dom::create_section();
    let _ = Dom::create_article();
    let _ = Dom::create_aside();
    let _ = Dom::create_nav();
    let _ = Dom::create_main();
    let _ = Dom::create_figure();
    let _ = Dom::create_figcaption();
    let _ = Dom::create_address();
}

fn _coverage_no_arg_interactive() {
    let _ = Dom::create_details_no_a11y();
    let _ = Dom::create_summary_no_a11y();
    let _ = Dom::create_dialog_no_a11y();
}

fn _coverage_no_arg_headings() {
    let _ = Dom::create_h1();
    let _ = Dom::create_h2();
    let _ = Dom::create_h3();
    let _ = Dom::create_h4();
    let _ = Dom::create_h5();
    let _ = Dom::create_h6();
}

fn _coverage_no_arg_text_block() {
    let _ = Dom::create_p();
    let _ = Dom::create_pre();
    let _ = Dom::create_blockquote();
    let _ = Dom::create_br();
    let _ = Dom::create_hr();
}

fn _coverage_no_arg_inline_text() {
    let _ = Dom::create_span();
    let _ = Dom::create_strong();
    let _ = Dom::create_em();
    let _ = Dom::create_b();
    let _ = Dom::create_i();
    let _ = Dom::create_u();
    let _ = Dom::create_s();
    let _ = Dom::create_small();
    let _ = Dom::create_big();
    let _ = Dom::create_mark();
    let _ = Dom::create_del();
    let _ = Dom::create_ins();
    let _ = Dom::create_sub();
    let _ = Dom::create_sup();
    let _ = Dom::create_samp();
    let _ = Dom::create_kbd();
    let _ = Dom::create_var();
    let _ = Dom::create_cite();
    let _ = Dom::create_dfn();
    let _ = Dom::create_abbr();
    let _ = Dom::create_acronym();
    let _ = Dom::create_code();
    let _ = Dom::create_q();
    let _ = Dom::create_bdo();
    let _ = Dom::create_bdi();
    let _ = Dom::create_wbr();
}

fn _coverage_no_arg_ruby() {
    let _ = Dom::create_ruby();
    let _ = Dom::create_rt();
    let _ = Dom::create_rtc();
    let _ = Dom::create_rp();
}

fn _coverage_no_arg_lists() {
    let _ = Dom::create_ul();
    let _ = Dom::create_ol();
    let _ = Dom::create_li();
    let _ = Dom::create_dl();
    let _ = Dom::create_dt();
    let _ = Dom::create_dd();
    let _ = Dom::create_menu_no_a11y();
    let _ = Dom::create_menuitem_no_a11y();
    let _ = Dom::create_dir();
}

fn _coverage_no_arg_table() {
    let _ = Dom::create_caption();
    let _ = Dom::create_thead();
    let _ = Dom::create_tbody();
    let _ = Dom::create_tfoot();
    let _ = Dom::create_tr();
    let _ = Dom::create_th();
    let _ = Dom::create_td();
    let _ = Dom::create_colgroup();
}

fn _coverage_no_arg_form() {
    let _ = Dom::create_form_no_a11y();
    let _ = Dom::create_fieldset_no_a11y();
    let _ = Dom::create_legend_no_a11y();
    let _ = Dom::create_output_no_a11y();
    let _ = Dom::create_datalist_no_a11y();
}

fn _coverage_no_arg_embedded() {
    let _ = Dom::create_canvas_no_a11y();
    let _ = Dom::create_object();
    let _ = Dom::create_embed();
    let _ = Dom::create_audio_no_a11y();
    let _ = Dom::create_video_no_a11y();
    let _ = Dom::create_map();
    let _ = Dom::create_area_no_a11y();
    let _ = Dom::create_svg();
}

fn _coverage_no_arg_metadata() {
    let _ = Dom::create_title();
    let _ = Dom::create_meta();
    let _ = Dom::create_link();
    let _ = Dom::create_script();
    let _ = Dom::create_style();
}

// ---------------------------------------------------------------------------
// `_with_text` constructors (plus other text-bearing ctors with non-suffix names).
// ---------------------------------------------------------------------------

fn _coverage_with_text_headings() {
    let s = AzString::from("");
    let _ = Dom::create_h1_with_text(s.clone());
    let _ = Dom::create_h2_with_text(s.clone());
    let _ = Dom::create_h3_with_text(s.clone());
    let _ = Dom::create_h4_with_text(s.clone());
    let _ = Dom::create_h5_with_text(s.clone());
    let _ = Dom::create_h6_with_text(s);
}

fn _coverage_with_text_inline() {
    let s = AzString::from("");
    let _ = Dom::create_span_with_text(s.clone());
    let _ = Dom::create_strong_with_text(s.clone());
    let _ = Dom::create_em_with_text(s.clone());
    let _ = Dom::create_b_with_text(s.clone());
    let _ = Dom::create_i_with_text(s.clone());
    let _ = Dom::create_u_with_text(s.clone());
    let _ = Dom::create_s_with_text(s.clone());
    let _ = Dom::create_small_with_text(s.clone());
    let _ = Dom::create_big_with_text(s.clone());
    let _ = Dom::create_mark_with_text(s.clone());
    let _ = Dom::create_del_with_text(s.clone());
    let _ = Dom::create_ins_with_text(s.clone());
    let _ = Dom::create_sub_with_text(s.clone());
    let _ = Dom::create_sup_with_text(s.clone());
    let _ = Dom::create_samp_with_text(s.clone());
    let _ = Dom::create_kbd_with_text(s.clone());
    let _ = Dom::create_var_with_text(s.clone());
    let _ = Dom::create_cite_with_text(s.clone());
    let _ = Dom::create_dfn_with_text(s.clone());
    let _ = Dom::create_acronym_with_text(s.clone());
    let _ = Dom::create_code_with_text(s.clone());
    let _ = Dom::create_bdi_with_text(s.clone());
    let _ = Dom::create_bdo_with_text(s);
}

fn _coverage_with_text_block() {
    let s = AzString::from("");
    let _ = Dom::create_p_with_text(s.clone());
    let _ = Dom::create_pre_with_text(s.clone());
    let _ = Dom::create_blockquote_with_text(s.clone());
    let _ = Dom::create_summary_with_text_no_a11y(s.clone());
    let _ = Dom::create_li_with_text(s.clone());
    let _ = Dom::create_td_with_text(s.clone());
    let _ = Dom::create_th_with_text(s.clone());
    let _ = Dom::create_th_with_scope(AzString::from("col"), s.clone());
    let _ = Dom::create_menuitem_with_text_no_a11y(s.clone());
    let _ = Dom::create_title_with_text(s.clone());
    let _ = Dom::create_style_with_text(s);
}

fn _coverage_with_text_ruby() {
    let s = AzString::from("");
    let _ = Dom::create_rt_with_text(s.clone());
    let _ = Dom::create_rp_with_text(s);
}

// ---------------------------------------------------------------------------
// Constructors that take semantically meaningful args (other than just text).
// ---------------------------------------------------------------------------

fn _coverage_args() {
    let s = AzString::from("");
    let _ = Dom::create_text(s.clone());
    let _ = Dom::create_icon(s.clone());
    let _ = Dom::create_abbr_with_title(s.clone(), s.clone());
    let _ = Dom::create_time(s.clone(), OptionString::None);
    let _ = Dom::create_option_no_a11y(s.clone(), s.clone());
    let _ = Dom::create_optgroup_no_a11y(s.clone());
    let _ = Dom::create_col(0);
    let _ = Dom::create_progress_no_a11y(0.0, 1.0);
    let _ = Dom::create_meter_no_a11y(0.0, 0.0, 1.0);
    let _ = Dom::create_param(s.clone(), s.clone());
    let _ = Dom::create_source(s.clone(), s.clone());
    let _ = Dom::create_track(s.clone(), s.clone());
    let _ = Dom::create_base(s.clone());
    let _ = Dom::create_data(s.clone());
    let _ = Dom::create_data_with_text(s.clone(), s);
}

// ---------------------------------------------------------------------------
// `_no_a11y` escape-hatch constructors.
// ---------------------------------------------------------------------------

fn _coverage_no_a11y() {
    let s = AzString::from("");
    let _ = Dom::create_a_no_a11y(s.clone(), OptionString::None);
    let _ = Dom::create_button_no_a11y(s.clone());
    let _ = Dom::create_label_no_a11y(s.clone(), s.clone());
    let _ = Dom::create_input_no_a11y(s.clone(), s.clone(), s.clone());
    let _ = Dom::create_textarea_no_a11y(s.clone(), s.clone());
    let _ = Dom::create_select_no_a11y(s.clone(), s.clone());
    let _ = Dom::create_table_no_a11y();
}

// ---------------------------------------------------------------------------
// Accessibility-aware constructors (paired with the `_no_a11y` variants above).
// ---------------------------------------------------------------------------

fn _coverage_a11y() {
    let s = AzString::from("");
    let aria = SmallAriaInfo::label("");
    let _ = Dom::create_a(s.clone(), s.clone(), aria.clone());
    let _ = Dom::create_button(s.clone(), aria.clone());
    let _ = Dom::create_label(s.clone(), s.clone(), aria.clone());
    let _ = Dom::create_input(s.clone(), s.clone(), s.clone(), aria.clone());
    let _ = Dom::create_textarea(s.clone(), s.clone(), aria.clone());
    let _ = Dom::create_select(s.clone(), s.clone(), aria.clone());
    let _ = Dom::create_table(s, aria);
}

// ---------------------------------------------------------------------------
// Generic / conversion constructors.
// ---------------------------------------------------------------------------

fn _coverage_generic(image: ImageRef, refany: RefAny, callback: VirtualViewCallback) {
    let _ = Dom::create_node(NodeType::Div);
    let _ = Dom::create_from_data(NodeData::create_div());
    let _ = Dom::create_image(image);
    let _ = Dom::create_virtual_view(refany, callback);
}

#[test]
fn dom_constructor_coverage() {
    // The test passes as long as the file compiles. The bodies above never
    // run; their job is to force a compile error if any constructor is
    // accidentally removed or renamed.
}
