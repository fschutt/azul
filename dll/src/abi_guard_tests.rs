//! The ABI guard's runtime half (the generated `AzAbi_getHash` export and the
//! `az_abi_check_hash` every binding runs before its first call into
//! libazul; generator: `doc/src/codegen/v2/abi_guard.rs`).
//!
//! On 2026-09-30 apps linked against a libazul built from another api.json
//! misread its structs (one AzWidgets reached 17.7 GB). The guard turns that
//! into an immediate abort with a message naming both hashes.

use crate::ffi::dll::{az_abi_check_hash, az_abi_mismatch_message, AzAbi_getHash, AZ_ABI_HASH};

/// Set in the child process the abort test spawns.
const CHILD_ENV: &str = "AZ_ABI_GUARD_TEST_CHILD";

#[test]
fn libazul_exports_the_abi_hash_its_bindings_compare_against() {
    assert_eq!(AzAbi_getHash(), AZ_ABI_HASH);
    assert_ne!(AZ_ABI_HASH, 0, "the codegen must compute a real hash");
    assert_eq!(az_abi_mismatch_message(AZ_ABI_HASH, AzAbi_getHash()), None);
    // A matching library passes the check (and the process lives on).
    az_abi_check_hash(AZ_ABI_HASH);
}

#[test]
fn the_mismatch_message_names_both_hashes_and_says_to_rebuild_the_app() {
    let message = az_abi_mismatch_message(0x1111_2222_3333_4444, 0xaaaa_bbbb_cccc_dddd)
        .expect("different hashes are a mismatch");
    assert!(message.contains("1111222233334444"), "{message}");
    assert!(message.contains("aaaabbbbccccdddd"), "{message}");
    assert!(
        message.contains("rebuild the app against this libazul"),
        "{message}"
    );
}

#[test]
fn a_libazul_with_another_abi_hash_aborts_the_app_with_both_hashes() {
    let wrong = AZ_ABI_HASH ^ 0x5a5a;
    if std::env::var_os(CHILD_ENV).is_some() {
        // The child: the guard must abort here, before the exit below.
        az_abi_check_hash(wrong);
        std::process::exit(0);
    }
    // The parent: run exactly this test again in a child process (an abort
    // would take the test harness down with it).
    let path = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, p)| p);
    let test = format!("{path}::a_libazul_with_another_abi_hash_aborts_the_app_with_both_hashes");
    let out = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([test.as_str(), "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .expect("spawn the child test process");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a mismatched libazul must abort the app, it exited cleanly; stderr:\n{stderr}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            out.status.signal(),
            Some(6),
            "the guard aborts (SIGABRT); status {:?}, stderr:\n{stderr}",
            out.status
        );
    }
    assert!(stderr.contains(&format!("{AZ_ABI_HASH:016x}")), "{stderr}");
    assert!(stderr.contains(&format!("{wrong:016x}")), "{stderr}");
    assert!(
        stderr.contains("rebuild the app against this libazul"),
        "{stderr}"
    );
}
