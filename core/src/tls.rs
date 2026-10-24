//! Which certificate authorities a TLS connection trusts: the public web PKI by default, or a
//! set the app pins for a far end it knows (its token server, its relay, its database).
//!
//! The same value configures every TLS client of the framework: layout's HTTP client
//! (`HttpRequestConfig`, `HttpClientConfig`), a database's backup sync (`DbConfig`) and the iroh
//! endpoint's relays (`IrohConfig`). A pinned set replaces the public one: a certificate from any
//! other authority - one issued wrongly by any of the ~150 public roots - fails the handshake.
//! On web the browser decides: `fetch` cannot pin, so a pinned set does not apply there.

use azul_css::StringVec;

/// The certificate authorities a TLS connection trusts.
// repr(C, u8) FFI enum: boxing the pinned list would change the C ABI of every binding.
#[allow(variant_size_differences)]
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TrustRoots {
    /// The public web PKI: Mozilla's roots, compiled in. The default.
    #[default]
    PublicRoots,
    /// Only these roots, PEM text (each entry one or more `CERTIFICATE` blocks). An entry that
    /// does not parse makes every connection fail and say so; it never falls back to the
    /// public roots.
    Pinned(StringVec),
}

impl TrustRoots {
    /// The public web PKI (the default).
    #[must_use]
    pub const fn public_roots() -> Self {
        TrustRoots::PublicRoots
    }

    /// Only the roots in `pems` (PEM text).
    #[must_use]
    pub const fn pinned(pems: StringVec) -> Self {
        TrustRoots::Pinned(pems)
    }

    /// Whether a set is pinned (anything but the public roots).
    #[must_use]
    pub const fn is_pinned(&self) -> bool {
        matches!(self, TrustRoots::Pinned(_))
    }
}

#[cfg(all(test, feature = "tls-testing", not(target_arch = "wasm32")))]
mod tests {
    use azul_css::{AzString, StringVec};

    use super::*;
    use crate::tls_testing::TestCa;

    fn pems(cas: &[&TestCa]) -> StringVec {
        StringVec::from_vec(cas.iter().map(|ca| AzString::from(ca.cert_pem.clone())).collect())
    }

    fn der(cas: &[&TestCa]) -> Vec<RootCertificate> {
        cas.iter().map(|ca| RootCertificate::from(ca.cert_der.clone())).collect()
    }

    #[test]
    fn the_public_roots_are_mozillas_and_a_pinned_set_is_exactly_its_roots() {
        let public = TrustRoots::public_roots().certificates().expect("public");
        assert_eq!(public.len(), webpki_root_certs::TLS_SERVER_ROOT_CERTS.len());
        assert!(public.len() > 100, "the compiled-in Mozilla set");
        let a = TestCa::new("azul test root A");
        let b = TestCa::new("azul test root B");
        let pinned = TrustRoots::pinned(pems(&[&a, &b])).certificates().expect("pinned");
        assert_eq!(pinned, der(&[&a, &b]));
    }

    #[test]
    fn a_distrusted_root_leaves_the_set_and_every_other_root_stays() {
        let a = TestCa::new("azul test root A");
        let b = TestCa::new("azul test root B");
        let c = TestCa::new("azul test root C");
        let public = der(&[&a, &b, &c]);
        let set = TrustRoots::public_roots_except(pems(&[&b]))
            .certificates_from(&public)
            .expect("public minus b");
        assert_eq!(set, der(&[&a, &c]));
        let set = TrustRoots::pinned(pems(&[&a, &b]))
            .with_distrusted(pems(&[&a]))
            .certificates()
            .expect("pinned minus a");
        assert_eq!(set, der(&[&b]));
    }

    #[test]
    fn a_distrusted_mozilla_root_leaves_the_compiled_in_set() {
        let first = webpki_root_certs::TLS_SERVER_ROOT_CERTS[0].clone();
        let pem = pem_certificate(first.as_ref());
        let set = TrustRoots::public_roots_except(StringVec::from_vec(vec![AzString::from(pem)]))
            .certificates()
            .expect("public minus one");
        assert_eq!(set.len(), webpki_root_certs::TLS_SERVER_ROOT_CERTS.len() - 1);
        assert!(!set.contains(&first));
    }

    #[test]
    fn the_pem_export_is_the_set_a_client_of_the_apps_own_loads() {
        let a = TestCa::new("azul test root A");
        let b = TestCa::new("azul test root B");
        let exported = TrustRoots::pinned(pems(&[&a, &b]))
            .with_distrusted(pems(&[&b]))
            .get_certificates_pem()
            .into_result()
            .expect("pem");
        assert_eq!(TrustRoots::pinned(exported).certificates().expect("reloads"), der(&[&a]));
        let public = TrustRoots::public_roots().get_certificates_pem().into_result().expect("pem");
        assert_eq!(public.as_ref().len(), webpki_root_certs::TLS_SERVER_ROOT_CERTS.len());
    }

    #[test]
    fn a_broken_entry_fails_the_set_and_never_falls_back_to_the_public_roots() {
        let broken = StringVec::from_vec(vec![AzString::from("not a pem")]);
        let why = TrustRoots::pinned(broken.clone()).certificates().unwrap_err();
        assert!(why.contains("trusted root 0"), "{why}");
        let why = TrustRoots::public_roots_except(broken).certificates().unwrap_err();
        assert!(why.contains("distrusted root 0"), "{why}");
    }

    #[test]
    fn a_config_without_a_set_takes_the_apps_and_adds_its_own_distrusted_roots() {
        let a = TestCa::new("azul test root A");
        let b = TestCa::new("azul test root B");
        let c = TestCa::new("azul test root C");
        // The app's setting is process-wide: this is the one test of the crate that sets it.
        set_app_trust_roots(TrustRoots::pinned(pems(&[&a, &b, &c])).with_distrusted(pems(&[&c])));
        let inherited = TrustRoots::app_default().certificates().expect("the app's");
        assert_eq!(inherited, der(&[&a, &b]));
        let narrowed = TrustRoots::app_default()
            .with_distrusted(pems(&[&a]))
            .certificates()
            .expect("the app's minus a");
        assert_eq!(narrowed, der(&[&b]));
        assert!(TrustRoots::default().is_pinned());
        assert!(!TrustRoots::default().is_public_roots());
        assert!(TrustRoots::public_roots().is_public_roots());
        assert!(!TrustRoots::public_roots_except(pems(&[&a])).is_public_roots());
        let own = TrustRoots::public_roots().certificates().expect("its own");
        assert_eq!(own.len(), webpki_root_certs::TLS_SERVER_ROOT_CERTS.len());
        set_app_trust_roots(TrustRoots::app_default());
        assert_eq!(app_trust_roots(), TrustRoots::public_roots());
    }
}
