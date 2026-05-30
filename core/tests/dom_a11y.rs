//! Compile-time coverage gate for the a11y soft-force constructor pattern.
//!
//! Every interactive Dom element that takes accessibility info has two
//! constructors:
//!
//! - `Dom::create_X(<a11y_struct>)` — the primary form; the required
//!   argument soft-forces the developer to supply a11y at the call site.
//! - `Dom::create_X_no_a11y(...)` — explicit opt-out; the longer name
//!   signals that the omission was intentional.
//!
//! For elements with type-specific a11y semantics there's a tailored
//! struct (`ProgressAriaInfo`, `MeterAriaInfo`, `DialogAriaInfo`). Other
//! interactive elements use the generic `SmallAriaInfo`.
//!
//! The test never executes its bodies; if any of the listed constructors
//! or struct builder methods is removed or renamed, this file stops
//! compiling and CI catches the regression.

#![allow(unused_imports, dead_code, unused_variables, clippy::all)]

use azul_core::a11y::{
    DialogAriaInfo, MeterAriaInfo, ProgressAriaInfo, SmallAriaInfo,
};
use azul_core::dom::Dom;
use azul_css::{AzString, OptionString};

// ---------------------------------------------------------------------------
// Type-relevant a11y struct constructors and builder methods.
// ---------------------------------------------------------------------------

fn _coverage_progress_aria_info() {
    let _ = ProgressAriaInfo::create(AzString::from("Upload"))
        .with_current_value(0.6)
        .with_max(1.0)
        .with_indeterminate(false)
        .with_description(AzString::from("File upload"));
}

fn _coverage_meter_aria_info() {
    let _ = MeterAriaInfo::create(AzString::from("Disk"), 0.75, 0.0, 1.0)
        .with_low(0.2)
        .with_high(0.8)
        .with_optimum(0.5)
        .with_description(AzString::from("Disk usage"));
}

fn _coverage_dialog_aria_info() {
    let _ = DialogAriaInfo::create(AzString::from("Settings"))
        .with_modal(true)
        .with_described_by(AzString::from("settings-desc"))
        .with_description(AzString::from("Application settings"));
}

fn _coverage_small_aria_info_builders() {
    let _ = SmallAriaInfo::label("Save")
        .with_description("Save the current document");
}

// ---------------------------------------------------------------------------
// Existing interactive elements (the original 7).
// ---------------------------------------------------------------------------

fn _coverage_existing_pairs() {
    // The widget-level `Dom::create_*` ctors take text plus aria.

    let _ = Dom::create_a_no_a11y(AzString::from("/"), OptionString::None);
    let _ = Dom::create_a("/", "Home", SmallAriaInfo::label("Home"));

    let _ = Dom::create_button_no_a11y(AzString::from("Save"));
    let _ = Dom::create_button("Save", SmallAriaInfo::label("Save"));

    let _ = Dom::create_table_no_a11y();
    let _ = Dom::create_table("Sales", SmallAriaInfo::label("Sales"));

    let _ = Dom::create_input_no_a11y(
        AzString::from("text"),
        AzString::from("name"),
        AzString::from("Name"),
    );
    let _ = Dom::create_input(
        "text",
        "name",
        "Name",
        SmallAriaInfo::label("Name"),
    );

    let _ = Dom::create_textarea_no_a11y(
        AzString::from("body"),
        AzString::from("Body"),
    );
    let _ = Dom::create_textarea(
        "body",
        "Body",
        SmallAriaInfo::label("Body"),
    );

    let _ = Dom::create_select_no_a11y(
        AzString::from("country"),
        AzString::from("Country"),
    );
    let _ = Dom::create_select(
        "country",
        "Country",
        SmallAriaInfo::label("Country"),
    );

    let _ = Dom::create_label_no_a11y(AzString::from("name"), AzString::from("Name"));
    let _ = Dom::create_label(
        "name",
        "Name",
        SmallAriaInfo::label("Name"),
    );
}

// ---------------------------------------------------------------------------
// Type-specific a11y elements (progress, meter, dialog).
// ---------------------------------------------------------------------------

fn _coverage_progress() {
    let _ = Dom::create_progress_no_a11y(0.5, 1.0);
    let _ = Dom::create_progress(
        ProgressAriaInfo::create(AzString::from("Upload"))
            .with_current_value(0.5)
            .with_max(1.0),
    );
}

fn _coverage_meter() {
    let _ = Dom::create_meter_no_a11y(0.75, 0.0, 1.0);
    let _ = Dom::create_meter(MeterAriaInfo::create(
        AzString::from("Disk"),
        0.75,
        0.0,
        1.0,
    ));
}

fn _coverage_dialog() {
    let _ = Dom::create_dialog_no_a11y();
    let _ = Dom::create_dialog(
        DialogAriaInfo::create(AzString::from("Confirm")).with_modal(true),
    );
}

// ---------------------------------------------------------------------------
// Generic a11y elements (use SmallAriaInfo).
// ---------------------------------------------------------------------------

fn _coverage_disclosure() {
    let _ = Dom::create_details_no_a11y();
    let _ = Dom::create_details(SmallAriaInfo::label("Advanced"));

    let _ = Dom::create_summary_no_a11y();
    let _ = Dom::create_summary(SmallAriaInfo::label("More"));

    let _ = Dom::create_summary_with_text_no_a11y("More");
    let _ = Dom::create_summary_with_text("More", SmallAriaInfo::label("More"));
}

fn _coverage_form_grouping() {
    let _ = Dom::create_form_no_a11y();
    let _ = Dom::create_form(SmallAriaInfo::label("Login"));

    let _ = Dom::create_fieldset_no_a11y();
    let _ = Dom::create_fieldset(SmallAriaInfo::label("Personal"));

    let _ = Dom::create_legend_no_a11y();
    let _ = Dom::create_legend(SmallAriaInfo::label("Address"));

    let _ = Dom::create_output_no_a11y();
    let _ = Dom::create_output(SmallAriaInfo::label("Total"));
}

fn _coverage_media() {
    let _ = Dom::create_audio_no_a11y();
    let _ = Dom::create_audio(SmallAriaInfo::label("Track"));

    let _ = Dom::create_video_no_a11y();
    let _ = Dom::create_video(SmallAriaInfo::label("Clip"));

    let _ = Dom::create_canvas_no_a11y();
    let _ = Dom::create_canvas(SmallAriaInfo::label("Drawing"));
}

fn _coverage_image_map() {
    let _ = Dom::create_area_no_a11y();
    let _ = Dom::create_area(SmallAriaInfo::label("Region"));
}

fn _coverage_select_options() {
    let _ = Dom::create_option_no_a11y(AzString::from("us"), AzString::from("USA"));
    let _ = Dom::create_option(
        AzString::from("us"),
        AzString::from("USA"),
        SmallAriaInfo::label("United States"),
    );

    let _ = Dom::create_optgroup_no_a11y(AzString::from("North America"));
    let _ = Dom::create_optgroup(
        AzString::from("North America"),
        SmallAriaInfo::label("North America"),
    );

    let _ = Dom::create_datalist_no_a11y();
    let _ = Dom::create_datalist(SmallAriaInfo::label("Suggestions"));
}

fn _coverage_menu() {
    let _ = Dom::create_menu_no_a11y();
    let _ = Dom::create_menu(SmallAriaInfo::label("File"));

    let _ = Dom::create_menuitem_no_a11y();
    let _ = Dom::create_menuitem(SmallAriaInfo::label("Open"));

    let _ = Dom::create_menuitem_with_text_no_a11y("Open");
    let _ = Dom::create_menuitem_with_text("Open", SmallAriaInfo::label("Open"));
}

#[test]
fn dom_a11y_constructor_coverage() {
    // The test passes purely by compiling.
}
