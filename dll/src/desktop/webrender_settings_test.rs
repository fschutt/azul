use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_is_the_default_and_the_default_is_production() {
    let none = WebRenderSettings::from_lookup(&NoEnv);
    assert_eq!(none, WebRenderSettings::default());
    // off by default
    assert!(!none.hit_test_items);
    assert!(!none.walk_gpu_values);
    assert!(!none.sync_scene);
    assert!(!none.hit_tester);
    // ON by default: the variable turns them off
    assert!(none.view_slice);
    assert!(none.zoom_tiles);
    // the numbers the variables name
    assert_eq!(none.atlas_size, 1024);
    assert_eq!(none.image_budget_mb, 256);
    assert_eq!(none.glyph_budget_mb, 16);
}

#[test]
fn the_switches_that_are_on_by_default_go_off_for_a_zero_and_only_for_a_zero() {
    for (value, on) in [("0", false), ("1", true), ("", true), ("off", true), ("00", true)] {
        let env = MapEnv::new()
            .with("AZ_WR_VIEW_SLICE", value)
            .with("AZ_WR_ZOOM_TILES", value);
        let settings = WebRenderSettings::from_lookup(&env);
        assert_eq!(settings.view_slice, on, "AZ_WR_VIEW_SLICE={value:?}");
        assert_eq!(settings.zoom_tiles, on, "AZ_WR_ZOOM_TILES={value:?}");
    }
}

#[test]
fn the_hit_test_items_need_exactly_a_one() {
    for (value, on) in [("1", true), ("0", false), ("", false), ("true", false), ("2", false)] {
        let settings =
            WebRenderSettings::from_lookup(&MapEnv::new().with("AZ_WR_HIT_TEST", value));
        assert_eq!(settings.hit_test_items, on, "AZ_WR_HIT_TEST={value:?}");
    }
}

#[test]
fn the_per_frame_switches_need_a_value_that_is_not_empty_and_not_zero() {
    for (value, on) in [("1", true), ("yes", true), ("0", false), ("", false)] {
        let env = MapEnv::new()
            .with("AZ_WALK_GPU_VALUES", value)
            .with("AZ_WR_SYNC_SCENE", value);
        let settings = WebRenderSettings::from_lookup(&env);
        assert_eq!(settings.walk_gpu_values, on, "AZ_WALK_GPU_VALUES={value:?}");
        assert_eq!(settings.sync_scene, on, "AZ_WR_SYNC_SCENE={value:?}");
    }
}

#[test]
fn the_hit_tester_is_on_when_the_variable_is_set_at_all() {
    for value in ["", "0", "1"] {
        let settings = WebRenderSettings::from_lookup(&MapEnv::new().with("AZ_WR_HIT_TESTER", value));
        assert!(settings.hit_tester, "AZ_WR_HIT_TESTER={value:?}");
    }
}

#[test]
fn the_atlas_size_is_clamped_and_falls_back_on_a_word() {
    for (value, size) in [
        ("2048", 2048),
        ("100", 512),
        ("-5", 512),
        ("99999", 4096),
        ("big", 1024),
        ("", 1024),
    ] {
        let settings = WebRenderSettings::from_lookup(&MapEnv::new().with("AZ_WR_ATLAS_SIZE", value));
        assert_eq!(settings.atlas_size, size, "AZ_WR_ATLAS_SIZE={value:?}");
    }
}

#[test]
fn the_budgets_are_megabytes_capped_at_a_gigabyte_and_zero_is_webrenders_rule() {
    for (value, image, glyph) in [
        ("64", 64, 64),
        ("0", 0, 0),
        ("5000", 1024, 1024),
        ("lots", 256, 16),
        ("-1", 256, 16),
    ] {
        let env = MapEnv::new()
            .with("AZ_WR_IMAGE_BUDGET_MB", value)
            .with("AZ_WR_GLYPH_BUDGET_MB", value);
        let settings = WebRenderSettings::from_lookup(&env);
        assert_eq!(settings.image_budget_mb, image, "image budget {value:?}");
        assert_eq!(settings.glyph_budget_mb, glyph, "glyph budget {value:?}");
    }
}
