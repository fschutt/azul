use azul_css::env::MapEnv;

use super::*;
use crate::settings::{PlatformEnv, ServiceSettings};

fn platform(vars: &[(&str, &str)]) -> PlatformEnv {
    let mut env = MapEnv::new();
    for (name, value) in vars {
        env.set(name, value);
    }
    PlatformEnv::from_lookup(&env)
}

#[test]
fn a_snap_is_a_snap_wherever_it_lives_and_the_rest_falls_through() {
    // the marker decides before the path is looked at
    assert_eq!(
        InstallKind::detect_with(Path::new("/usr/bin/azwriter"), true),
        InstallKind::Snap
    );
    assert_eq!(
        InstallKind::detect_with(Path::new("/usr/bin/azwriter"), false),
        InstallKind::SystemPackageManager
    );
    // what the app read at its start is the answer of detect_in
    let snap = platform(&[("SNAP", "/snap/azwriter/1")]);
    assert_eq!(InstallKind::detect_in(&snap), InstallKind::Snap);
    assert_ne!(
        InstallKind::detect_in(&PlatformEnv::default()),
        InstallKind::Snap
    );
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[test]
fn the_state_folder_is_under_the_data_home_the_platform_values_name() {
    let xdg = platform(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "/xdg")]);
    assert_eq!(
        private_state_dir(&xdg, "azwriter"),
        Ok(PathBuf::from("/xdg/azwriter"))
    );
    let home_only = platform(&[("HOME", "/home/u")]);
    assert_eq!(
        private_state_dir(&home_only, "azwriter"),
        Ok(PathBuf::from("/home/u/.local/share/azwriter"))
    );
    // no data folder, no private folder: the shared temporary folder is never the answer
    let none = PlatformEnv::default();
    assert!(private_state_dir(&none, "azwriter").is_err());
    // a relative XDG folder is no folder
    let relative = platform(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "data")]);
    assert_eq!(
        private_state_dir(&relative, "azwriter"),
        Ok(PathBuf::from("/home/u/.local/share/azwriter"))
    );
}

#[test]
fn the_default_state_folder_falls_back_to_the_temp_folder_only_for_what_is_a_convenience() {
    let none = PlatformEnv::default();
    assert_eq!(
        default_state_dir(&none, "azwriter"),
        std::env::temp_dir().join("azwriter")
    );
}

#[test]
fn a_forced_cohort_is_answered_without_being_kept() {
    let mut state = UpdateState {
        forced_bucket: Some(7),
        ..UpdateState::default()
    };
    assert_eq!(state.rollout_bucket(), 7);
    assert_eq!(state.rollout_bucket, None, "a drill is not a cohort the machine keeps");
    // without it the cohort is drawn once and kept
    let mut drawn = UpdateState::default();
    let bucket = drawn.rollout_bucket();
    assert!(bucket < 100);
    assert_eq!(drawn.rollout_bucket, Some(bucket));
    assert_eq!(drawn.rollout_bucket(), bucket);
}

#[test]
fn the_app_env_takes_the_platform_and_the_cohort_from_the_services_settings() {
    let services = ServiceSettings {
        platform: platform(&[("HOME", "/home/u")]),
        update_bucket: Some(42),
        ..ServiceSettings::default()
    };
    let env = crate::appenv::AppEnv::default().with_services(&services);
    assert_eq!(env.platform, services.platform);
    assert_eq!(env.update_bucket, Some(42));
    // the config's part is untouched
    assert_eq!(env.app_name, crate::appenv::AppEnv::default().app_name);
    let plain = crate::appenv::AppEnv::default();
    assert_eq!(plain.platform, PlatformEnv::default());
    assert_eq!(plain.update_bucket, None);
}
