//! X-AR5a-7: `LocalDrive` overrides most of the `Drive` methods; the others fall to the
//! trait's defaults. Which ones is a decision, written down here with its reason: a new `Drive`
//! method fails this test until it is either overridden by `LocalDrive` or listed with the reason
//! the default is right for a folder on disk, and `forward_drive!` must forward it.

/// The methods `LocalDrive` leaves to the trait's default, each with why that is right.
const LOCAL_DEFAULTS: &[(&str, &str)] = &[
    ("resume_folder", "a folder on disk has no multipart state: None"),
    ("put_file", "streams the file through LocalDrive::put_from"),
    ("put_file_checked", "opens the checked file once, streams it through LocalDrive::put_from"),
    ("put_many", "one LocalDrive::put after the other: a folder packs nothing"),
    ("delete_many", "one LocalDrive::delete after the other, a version checked by LocalDrive::head's tag"),
    ("put_from_many", "one LocalDrive::put_from after the other"),
];

fn methods_of(source: &str, header: &str) -> Vec<String> {
    let start = source
        .find(header)
        .unwrap_or_else(|| panic!("{header:?} not found"));
    let body = &source[start..];
    // The impl or trait ends at the first closing brace in column 0.
    let end = body.find("\n}\n").expect("end of the block");
    body[..end]
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("    fn ")
                .or_else(|| line.strip_prefix("            fn "))?;
            Some(rest.split(['(', '<']).next()?.trim().to_string())
        })
        .collect()
}

#[test]
fn x_ar5a_7_every_drive_method_is_overridden_by_the_local_drive_or_defaulted_for_a_reason() {
    let lib = include_str!("../lib.rs");
    let local = include_str!("../local.rs");
    let all = methods_of(lib, "pub trait Drive: Send + Sync {");
    assert!(all.len() >= 22, "the trait was parsed: {all:?}");
    let own = methods_of(local, "impl Drive for LocalDrive {");
    for method in &all {
        let overridden = own.contains(method);
        let defaulted = LOCAL_DEFAULTS.iter().any(|(m, _)| m == method);
        assert!(
            overridden != defaulted,
            "Drive::{method}: LocalDrive must either override it or list it in LOCAL_DEFAULTS \
             with the reason the default is right (overridden: {overridden}, listed: {defaulted})"
        );
    }
    for (method, reason) in LOCAL_DEFAULTS {
        assert!(all.iter().any(|m| m == method), "{method} is no Drive method");
        assert!(!reason.is_empty());
    }
}

#[test]
fn x_ar5a_7_the_pointer_impls_forward_every_drive_method() {
    let lib = include_str!("../lib.rs");
    let all = methods_of(lib, "pub trait Drive: Send + Sync {");
    let start = lib.find("macro_rules! forward_drive").expect("the macro");
    let forwarded: Vec<String> = lib[start..]
        .lines()
        .skip(1)
        .take_while(|l| !l.starts_with("forward_drive!"))
        .filter_map(|line| {
            let rest = line.strip_prefix("            fn ")?;
            Some(rest.split(['(', '<']).next()?.trim().to_string())
        })
        .collect();
    for method in &all {
        assert!(
            forwarded.contains(method),
            "Drive::{method} is not forwarded by a Box, an Arc or a reference to a drive"
        );
    }
}
