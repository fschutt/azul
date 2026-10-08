//! The secrets file: entries under the OS keyring's names, errors that never show a value.

use azul_storage::testing::TempDir;

use crate::secrets::{credentials_entry, drive_token_entry, FileSecrets};

#[test]
fn a_secret_is_kept_under_its_keyring_name_and_errors_never_show_it() {
    let dir = TempDir::new("azcloud-secrets");
    let secrets = FileSecrets::new(dir.path().join("secrets.json"));
    assert_eq!(secrets.get("x").unwrap(), None);
    secrets
        .set(&drive_token_entry("d_1"), "dt_f.0.sesame")
        .unwrap();
    secrets.set(&credentials_entry("d_1"), "{}").unwrap();
    assert_eq!(
        secrets.get("azul-storage/azlin/d_1").unwrap().as_deref(),
        Some("dt_f.0.sesame")
    );
    assert_eq!(credentials_entry("d_1"), "azul-storage/s3/d_1");
    assert!(secrets.remove("azul-storage/azlin/d_1").unwrap());
    assert!(!secrets.remove("azul-storage/azlin/d_1").unwrap());
    assert!(!format!("{secrets:?}").contains("sesame"));
    std::fs::write(
        secrets.path(),
        r#"{"format": "other", "version": 1, "entries": {"a": "sesame"}}"#,
    )
    .unwrap();
    let err = secrets.get("a").unwrap_err().to_string();
    assert!(!err.contains("sesame"), "{err}");
    std::fs::write(secrets.path(), r#"{"format": "azcloud.secrets", "entries": "sesame"}"#)
        .unwrap();
    let err = secrets.get("a").unwrap_err().to_string();
    assert!(!err.contains("sesame"), "a file that does not parse is not quoted: {err}");
}
