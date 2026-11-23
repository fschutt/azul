//! Tests of the CPU renderer's settings: an empty environment is the default (the switches that
//! are on by default included), and every variable's absent, empty, garbage and valid value.

use std::path::PathBuf;

use azul_css::env::{MapEnv, NoEnv};

use super::*;

/// The settings of an environment that sets one variable.
fn asked(name: &str, value: &str) -> RenderSettings {
    RenderSettings::from_lookup(&MapEnv::new().with(name, value))
}

const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);

/// `Default` is the behaviour of an EMPTY environment: the four switches that are on by default
/// are on, and nothing else is.
///
/// Mutation: a derived `Default` (every bool false) fails the first four assertions.
#[test]
fn an_empty_environment_is_the_default_and_ships_the_production_paths() {
    let none = RenderSettings::from_lookup(&NoEnv);
    assert_eq!(none, RenderSettings::default());
    assert_eq!(PaintSwitches::from_lookup(&NoEnv), PaintSwitches::default());
    assert_eq!(none.paint, PaintSwitches::default());
    // on by default
    assert!(none.paint.turned_text_direct, "turned text is drawn directly");
    assert!(none.paint.item_grid, "the walk uses the grids of the levels");
    assert!(none.paint.dl_diff_refine, "the damage refinements are on");
    assert!(none.halo_slots, "a halo's picture takes the place of its stand-in");
    // off or unset
    assert_eq!(none.paint.debug_fill, None);
    assert!(!none.paint.debug_damage);
    assert!(!none.paint.clip_debug);
    assert!(!none.paint.blit_debug);
    assert!(!none.paint.patch_debug);
    assert!(!none.paint.map_debug);
    assert!(!none.full_in_layers);
    assert_eq!(none.threads, None);
    assert_eq!(none.retained_bytes, None);
    assert!(!none.halo_stats);
    assert_eq!(none.dump_display_lists, None);
    assert_eq!(none.dump_frames, None);
}

#[test]
fn a_switch_that_is_on_by_default_goes_off_with_its_own_values_only() {
    assert!(!asked("AZ_CPU_TURNED_TEXT", "group").paint.turned_text_direct);
    assert!(asked("AZ_CPU_TURNED_TEXT", "direct").paint.turned_text_direct);
    assert!(asked("AZ_CPU_TURNED_TEXT", "").paint.turned_text_direct);

    assert!(!asked("AZ_CPU_GRID", "off").paint.item_grid);
    assert!(!asked("AZ_CPU_GRID", "0").paint.item_grid);
    assert!(asked("AZ_CPU_GRID", "1").paint.item_grid);
    assert!(asked("AZ_CPU_GRID", "").paint.item_grid);
    assert!(asked("AZ_CPU_GRID", "OFF").paint.item_grid, "only `off` and `0` turn the grid off");

    assert!(!asked("AZ_DL_DIFF_REFINE", "0").paint.dl_diff_refine);
    assert!(!asked("AZ_DL_DIFF_REFINE", "off").paint.dl_diff_refine);
    assert!(!asked("AZ_DL_DIFF_REFINE", "OFF").paint.dl_diff_refine, "off in any case");
    assert!(asked("AZ_DL_DIFF_REFINE", "1").paint.dl_diff_refine);
    assert!(asked("AZ_DL_DIFF_REFINE", "garbage").paint.dl_diff_refine);
    assert!(asked("AZ_DL_DIFF_REFINE", "").paint.dl_diff_refine);

    assert!(!asked("AZ_HALO_SLOTS", "0").halo_slots);
    assert!(asked("AZ_HALO_SLOTS", "1").halo_slots);
    assert!(asked("AZ_HALO_SLOTS", "").halo_slots);
    assert!(asked("AZ_HALO_SLOTS", "off").halo_slots, "only `0` turns the slots off");
}

#[test]
fn a_trace_switch_is_on_when_its_variable_is_set_to_anything() {
    type Read = fn(&RenderSettings) -> bool;
    let traces: [(&str, Read); 5] = [
        ("AZ_DEBUG_DAMAGE", |s| s.paint.debug_damage),
        ("AZ_CLIP_DEBUG", |s| s.paint.clip_debug),
        ("AZ_BLIT_DEBUG", |s| s.paint.blit_debug),
        ("AZ_PATCH_DEBUG", |s| s.paint.patch_debug),
        ("AZ_MAP_DEBUG", |s| s.paint.map_debug),
    ];
    for (name, read) in traces {
        assert!(!read(&RenderSettings::default()), "{name} is off when unset");
        assert!(read(&asked(name, "1")), "{name}=1");
        assert!(read(&asked(name, "")), "{name} set to nothing is still set");
        assert!(read(&asked(name, "0")), "{name}=0 is still set");
    }
}

#[test]
fn the_debug_fill_is_a_colour_when_the_variable_is_set_and_nothing_when_it_is_not() {
    assert_eq!(asked("AZ_DEBUG_FILL", "1").paint.debug_fill, Some((255, 0, 0, 255)), "1 is red");
    assert_eq!(asked("AZ_DEBUG_FILL", "00ff00").paint.debug_fill, Some((0, 255, 0, 255)));
    assert_eq!(asked("AZ_DEBUG_FILL", "#0000ff").paint.debug_fill, Some((0, 0, 255, 255)));
    // set to `0` or to nothing: the knob is explicitly off, and the damaged region is cleared to
    // white, as the window's own clear colour would not be
    assert_eq!(asked("AZ_DEBUG_FILL", "0").paint.debug_fill, Some(WHITE));
    assert_eq!(asked("AZ_DEBUG_FILL", "").paint.debug_fill, Some(WHITE));
    // garbage is red: whoever asked for a debug fill gets a visible one
    assert_eq!(asked("AZ_DEBUG_FILL", "nonsense").paint.debug_fill, Some((255, 0, 0, 255)));
    assert_eq!(RenderSettings::default().paint.debug_fill, None);
}

/// The old test of the parser, kept: it pins the exact colours of every spelling.
///
/// NEGATIVE CONTROL: making `parse_damage_fill` return WHITE for every input fails the "1" and
/// hexadecimal cases.
#[test]
fn the_damage_fill_knob_changes_the_clear_colour() {
    assert_eq!(parse_damage_fill(None), WHITE, "unset ships as white");
    assert_eq!(parse_damage_fill(Some("0")), WHITE, "0 turns it off");
    assert_eq!(parse_damage_fill(Some("")), WHITE);

    assert_eq!(parse_damage_fill(Some("1")), (255, 0, 0, 255), "1 is red");
    assert_ne!(
        parse_damage_fill(Some("1")),
        WHITE,
        "the debug fill must not be white, or nothing is visible"
    );

    assert_eq!(parse_damage_fill(Some("00ff00")), (0, 255, 0, 255));
    assert_eq!(parse_damage_fill(Some("#0000ff")), (0, 0, 255, 255));
    // Garbage falls back to red rather than to white: the user asked for a debug fill, so give
    // them a visible one.
    assert_eq!(parse_damage_fill(Some("nonsense")), (255, 0, 0, 255));
}

#[test]
fn the_thread_count_of_the_environment_is_a_positive_number_or_nothing() {
    assert_eq!(parse_threads(None), None);
    assert_eq!(parse_threads(Some("")), None);
    assert_eq!(parse_threads(Some("0")), None);
    assert_eq!(parse_threads(Some("four")), None);
    assert_eq!(parse_threads(Some("1")), Some(1));
    assert_eq!(parse_threads(Some(" 3 ")), Some(3));

    assert_eq!(asked("AZ_CPU_THREADS", "1").threads, Some(1));
    assert_eq!(asked("AZ_CPU_THREADS", "8").threads, Some(8));
    assert_eq!(asked("AZ_CPU_THREADS", "0").threads, None);
    assert_eq!(asked("AZ_CPU_THREADS", "").threads, None);
    assert_eq!(asked("AZ_CPU_THREADS", "many").threads, None);
}

#[test]
fn the_retained_bound_is_a_number_of_megabytes_in_bytes() {
    assert_eq!(parse_megabytes(Some("0")), Some(0), "AZ_CPU_RETAINED_MB=0 keeps nothing");
    assert_eq!(parse_megabytes(Some("64")), Some(64 << 20));
    assert_eq!(parse_megabytes(None), None);
    assert_eq!(parse_megabytes(Some("")), None);
    assert_eq!(parse_megabytes(Some("lots")), None);
    let huge = usize::MAX.to_string();
    assert_eq!(parse_megabytes(Some(huge.as_str())), Some(usize::MAX), "it saturates");

    assert_eq!(asked("AZ_CPU_RETAINED_MB", "0").retained_bytes, Some(0));
    assert_eq!(asked("AZ_CPU_RETAINED_MB", "64").retained_bytes, Some(64 << 20));
    assert_eq!(asked("AZ_CPU_RETAINED_MB", "").retained_bytes, None);
}

#[test]
fn the_layers_halo_stats_and_dump_variables_mean_what_they_did() {
    assert!(asked("AZ_CPU_FULL", "layers").full_in_layers);
    assert!(!asked("AZ_CPU_FULL", "bands").full_in_layers);
    assert!(!asked("AZ_CPU_FULL", "").full_in_layers);

    assert!(asked("AZ_HALO_STATS", "1").halo_stats);
    assert!(asked("AZ_HALO_STATS", "yes").halo_stats);
    assert!(!asked("AZ_HALO_STATS", "0").halo_stats);
    assert!(!asked("AZ_HALO_STATS", "").halo_stats);

    assert_eq!(
        asked("AZ_DUMP_DL_DIR", "/tmp/dls").dump_display_lists,
        Some(PathBuf::from("/tmp/dls"))
    );
    assert_eq!(
        asked("AZ_DUMP_FRAME_DIR", "/tmp/frames").dump_frames,
        Some(PathBuf::from("/tmp/frames"))
    );
    assert_eq!(asked("AZ_DUMP_DL_DIR", "/tmp/dls").dump_frames, None, "each has its own variable");
}

/// A dump directory is a path: a name that is not text keeps its bytes (`var_os`).
#[cfg(unix)]
#[test]
fn a_dump_dir_that_is_not_text_is_kept() {
    use std::os::unix::ffi::OsStringExt;
    let name = std::ffi::OsString::from_vec(vec![b'/', b't', 0x80, b'x']);
    let env = MapEnv::new().with_os("AZ_DUMP_FRAME_DIR", name.clone());
    assert_eq!(
        RenderSettings::from_lookup(&env).dump_frames,
        Some(PathBuf::from(name))
    );
}
