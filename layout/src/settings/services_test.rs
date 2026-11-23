use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_is_the_default() {
    assert_eq!(ServiceSettings::from_lookup(&NoEnv), ServiceSettings::default());
    assert_eq!(ServiceSettings::from_lookup(&MapEnv::new()), ServiceSettings::default());
}

#[test]
fn the_default_names_no_picker_and_allows_addr2line() {
    let settings = ServiceSettings::default();
    assert_eq!(settings.file_picker, None);
    assert!(settings.probe_addr2line, "on unless AZ_PROBE_ADDR2LINE=0");
    assert_eq!(settings.platform, PlatformEnv::default());
}

#[test]
fn the_picker_is_read_as_written() {
    let native = ServiceSettings::from_lookup(&MapEnv::new().with(PICKER_VAR, "native"));
    assert_eq!(native.file_picker.as_deref(), Some("native"));
    let file = ServiceSettings::from_lookup(&MapEnv::new().with("AZUL_FILE_PICKER", "/c/picker.json"));
    assert_eq!(file.file_picker.as_deref(), Some("/c/picker.json"));
}

#[test]
fn addr2line_is_off_for_a_zero_and_only_for_a_zero() {
    let allowed = |value: &str| {
        ServiceSettings::from_lookup(&MapEnv::new().with("AZ_PROBE_ADDR2LINE", value))
            .probe_addr2line
    };
    assert!(!allowed("0"));
    assert!(allowed("1"));
    assert!(allowed(""));
    assert!(allowed("off"));
}

#[test]
fn the_platform_group_reads_the_platform_variables() {
    let env = MapEnv::new().with("HOME", "/home/u").with("SNAP", "1");
    let settings = ServiceSettings::from_lookup(&env);
    assert_eq!(settings.platform, PlatformEnv::from_lookup(&env));
    assert!(settings.platform.snap);
}

#[test]
fn the_forced_rollout_cohort_is_a_number_from_zero_to_ninety_nine() {
    let bucket = |value: &str| {
        ServiceSettings::from_lookup(&MapEnv::new().with("AZ_UPDATE_BUCKET", value)).update_bucket
    };
    assert_eq!(bucket("0"), Some(0));
    assert_eq!(bucket("99"), Some(99));
    assert_eq!(bucket("100"), None, "a cohort is 0-99");
    assert_eq!(bucket("256"), None, "no u8");
    assert_eq!(bucket("-1"), None);
    assert_eq!(bucket(""), None);
    assert_eq!(bucket("soon"), None);
    assert_eq!(ServiceSettings::default().update_bucket, None);
}
