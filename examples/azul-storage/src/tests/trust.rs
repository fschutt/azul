//! The authorities the Azlin apps trust for the iroh relays: Let's Encrypt's four roots, which
//! n0's relays and the Azlin nodes' relays are served under, and nothing of the public set.

use crate::trust::AZLIN_RELAY_ROOTS;

#[test]
fn the_relays_are_trusted_under_let_s_encrypt_s_four_roots_only() {
    assert_eq!(AZLIN_RELAY_ROOTS.len(), 4);
    for (name, pem) in AZLIN_RELAY_ROOTS {
        assert!(name.starts_with("ISRG Root "), "{name}");
        assert_eq!(pem.matches("-----BEGIN CERTIFICATE-----").count(), 1, "{name}");
        assert!(pem.trim_end().ends_with("-----END CERTIFICATE-----"), "{name}");
    }
    let names: Vec<&str> = AZLIN_RELAY_ROOTS.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["ISRG Root X1", "ISRG Root X2", "ISRG Root YE", "ISRG Root YR"]);
}
