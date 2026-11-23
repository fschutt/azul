use super::*;

#[test]
fn no_env_answers_nothing() {
    assert_eq!(NoEnv.var("HOME"), None);
    assert!(!NoEnv.is_set("HOME"));
    #[cfg(feature = "std")]
    assert_eq!(NoEnv.var_os("HOME"), None);
}

#[test]
fn map_env_answers_what_it_was_given_and_is_empty_by_default() {
    let empty = MapEnv::new();
    assert_eq!(empty.var("AZ_X"), None);
    assert!(!empty.is_set("AZ_X"));
    let env = MapEnv::new().with("AZ_X", "1").with("AZ_EMPTY", "");
    assert_eq!(env.var("AZ_X").as_deref(), Some("1"));
    assert_eq!(env.var("AZ_EMPTY").as_deref(), Some(""));
    assert!(env.is_set("AZ_EMPTY"), "an empty value is set");
    assert_eq!(env.var("AZ_Y"), None);
    let mut env = env;
    env.remove("AZ_X");
    assert!(!env.is_set("AZ_X"));
    env.set("AZ_X", "2");
    assert_eq!(env.var("AZ_X").as_deref(), Some("2"));
}

#[test]
fn a_closure_is_a_source() {
    let by_closure = |name: &str| (name == "AZ_A").then(|| String::from("a"));
    let dynamic: &dyn EnvSource = &by_closure;
    assert_eq!(dynamic.var("AZ_A").as_deref(), Some("a"));
    assert_eq!(dynamic.var("AZ_B"), None);
    assert!(dynamic.is_set("AZ_A"));
}

#[cfg(feature = "std")]
#[test]
fn a_path_keeps_its_os_value_and_var_os_sees_text_too() {
    let env = MapEnv::new()
        .with("AZ_TXT", "/tmp/x")
        .with_os("AZ_OS", "/tmp/y");
    assert_eq!(
        env.var_os("AZ_TXT"),
        Some(std::ffi::OsString::from("/tmp/x"))
    );
    assert_eq!(
        env.var_os("AZ_OS"),
        Some(std::ffi::OsString::from("/tmp/y"))
    );
    assert_eq!(env.var("AZ_OS"), None, "an OS-only value is not text");
    assert!(env.is_set("AZ_OS"));
    // setting text replaces an OS-only value of the same name, and the other way round
    let env = env.with("AZ_OS", "text");
    assert_eq!(env.var("AZ_OS").as_deref(), Some("text"));
    let env = env.with_os("AZ_OS", "bin");
    assert_eq!(env.var("AZ_OS"), None);
}

#[cfg(all(feature = "std", unix))]
#[test]
fn a_non_utf8_path_survives_var_os() {
    use std::os::unix::ffi::OsStringExt;
    let raw = std::ffi::OsString::from_vec(vec![b'/', 0xff, b'x']);
    let env = MapEnv::new().with_os("AZ_DIR", raw.clone());
    assert_eq!(env.var_os("AZ_DIR"), Some(raw));
    assert_eq!(env.var("AZ_DIR"), None);
}
