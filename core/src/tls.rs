//! Which certificate authorities a TLS connection trusts.
//!
//! The public web PKI by default, a set the app pins for a far end it knows (its token server,
//! its relay, its database), and in either case minus the roots it distrusts (a public root
//! that issued certificates it should not have).
//!
//! The same value configures every TLS client of the framework and its apps: layout's HTTP
//! client (`HttpRequestConfig`, `HttpClientConfig`), a database's backup sync (`DbConfig`), the
//! iroh endpoint's relays (`IrohConfig`) and the apps' own mail, storage and map clients. A
//! config that names no set uses the app's (`AppConfig::trust_roots`, see
//! [`set_app_trust_roots`]), so an app hardens every connection in one place and a single call
//! can still pin its own. A pinned set replaces the public one: a certificate from any other
//! authority fails the handshake. On web the browser decides: `fetch` cannot pin, so neither a
//! pinned set nor a distrusted root applies there.

use azul_css::{AzString, StringVec};

/// The set of roots a connection starts from, before its distrusted roots are taken out.
// repr(C, u8) FFI enum: boxing the pinned list would change the C ABI of every binding.
#[allow(variant_size_differences)]
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TrustRootSet {
    /// The app's set for every connection (`AppConfig::trust_roots`), the public roots when
    /// the app set none. The default, so one setting hardens the whole app.
    #[default]
    AppDefault,
    /// The public web PKI: Mozilla's roots, compiled in.
    PublicRoots,
    /// Only these roots, PEM text (each entry one or more `CERTIFICATE` blocks). An entry that
    /// does not parse makes every connection fail and say so; it never falls back to the
    /// public roots.
    Pinned(StringVec),
}

/// The certificate authorities a TLS connection trusts: a set of roots minus the distrusted
/// ones.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustRoots {
    /// The set the connection starts from: the app's (the default), the public roots, or a
    /// pinned set.
    pub roots: TrustRootSet,
    /// Roots taken out of that set, PEM text: a root is dropped when its public key is the
    /// key of one of these certificates, so a re-issued copy of a distrusted root is dropped
    /// too. A root of the app's set stays distrusted when a config adds its own. An entry
    /// that does not parse fails every connection, as a broken pinned set does; one that
    /// is not in the set changes nothing.
    pub distrusted: StringVec,
}

impl TrustRoots {
    /// The app's set (`AppConfig::trust_roots`), the public roots when it set none: the
    /// default of every config.
    #[must_use]
    pub const fn app_default() -> Self {
        Self {
            roots: TrustRootSet::AppDefault,
            distrusted: StringVec::from_const_slice(&[]),
        }
    }

    /// The public web PKI, whatever the app set.
    #[must_use]
    pub const fn public_roots() -> Self {
        Self {
            roots: TrustRootSet::PublicRoots,
            distrusted: StringVec::from_const_slice(&[]),
        }
    }

    /// Only the roots in `pems` (PEM text).
    #[must_use]
    pub const fn pinned(pems: StringVec) -> Self {
        Self {
            roots: TrustRootSet::Pinned(pems),
            distrusted: StringVec::from_const_slice(&[]),
        }
    }

    /// The public web PKI without the roots in `pems` (PEM text): the usual hardening when a
    /// public authority has lost the trust of the browsers before the compiled-in roots
    /// caught up.
    #[must_use]
    pub const fn public_roots_except(pems: StringVec) -> Self {
        Self {
            roots: TrustRootSet::PublicRoots,
            distrusted: pems,
        }
    }

    /// This set without the roots in `pems` (PEM text), in place of the ones it distrusted.
    #[must_use]
    pub fn with_distrusted(mut self, pems: StringVec) -> Self {
        self.distrusted = pems;
        self
    }

    /// Whether a set is pinned (the public roots are not part of it).
    #[must_use]
    pub fn is_pinned(&self) -> bool {
        matches!(self.resolved().roots, TrustRootSet::Pinned(_))
    }

    /// Whether a connection made now trusts the public roots as they are: no pinned set, no
    /// distrusted root. A TLS client that brings its own fixed root store (a database driver)
    /// can follow only this set, and refuses to connect over TLS under any other.
    #[must_use]
    pub fn is_public_roots(&self) -> bool {
        let resolved = self.resolved();
        matches!(resolved.roots, TrustRootSet::PublicRoots | TrustRootSet::AppDefault)
            && resolved.distrusted.as_ref().is_empty()
    }

    /// What a connection made now trusts: the app's set filled in for
    /// [`TrustRootSet::AppDefault`], its distrusted roots joined to this one's.
    #[must_use]
    pub fn resolved(&self) -> Self {
        let TrustRootSet::AppDefault = self.roots else {
            return self.clone();
        };
        let app = app_trust_roots();
        let mut distrusted = app.distrusted.as_ref().to_vec();
        distrusted.extend(self.distrusted.as_ref().iter().cloned());
        Self {
            roots: app.roots,
            distrusted: StringVec::from_vec(distrusted),
        }
    }
}

impl TrustRoots {
    /// The root certificates a connection made now trusts, PEM (one certificate each): for a
    /// TLS client of the app's own (an IMAP or SMTP client, another language's TLS library),
    /// so it trusts exactly what azul's clients trust. Fails when an entry does not parse, or
    /// in a build without TLS (web: the browser's roots apply).
    #[must_use]
    pub fn get_certificates_pem(&self) -> ResultStringVecString {
        #[cfg(all(feature = "tls-roots", not(target_arch = "wasm32")))]
        let pems = self.certificates().map(|certs| {
            certs
                .iter()
                .map(|cert| AzString::from(pem_certificate(cert.as_ref())))
                .collect::<Vec<_>>()
        });
        #[cfg(not(all(feature = "tls-roots", not(target_arch = "wasm32"))))]
        let pems: Result<alloc::vec::Vec<AzString>, alloc::string::String> =
            Err(alloc::string::String::from("this build makes no TLS connections"));
        match pems {
            Ok(pems) => ResultStringVecString::Ok(StringVec::from_vec(pems)),
            Err(why) => ResultStringVecString::Err(AzString::from(why)),
        }
    }
}

// FFI-safe result of `TrustRoots::get_certificates_pem`.
azul_css::impl_result!(
    StringVec,
    AzString,
    ResultStringVecString,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

/// The app's set, as [`set_app_trust_roots`] left it. Never `AppDefault`.
#[cfg(feature = "std")]
static APP_TRUST_ROOTS: std::sync::RwLock<Option<TrustRoots>> = std::sync::RwLock::new(None);

/// Makes `roots` the set of every connection whose config names none (`AppDefault`).
///
/// What `App::create` does with `AppConfig::trust_roots`; a process without an `App` (a
/// headless service, a command line tool) calls it once at start. A pooled client or an
/// endpoint made earlier keeps the set it was made with. `AppDefault` here means the public
/// roots.
#[cfg(feature = "std")]
pub fn set_app_trust_roots(roots: TrustRoots) {
    let roots = match roots.roots {
        TrustRootSet::AppDefault => TrustRoots {
            roots: TrustRootSet::PublicRoots,
            distrusted: roots.distrusted,
        },
        _ => roots,
    };
    let mut slot = APP_TRUST_ROOTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = Some(roots);
}

/// The app's set: what [`set_app_trust_roots`] set, else the public roots.
#[cfg(feature = "std")]
#[must_use]
pub fn app_trust_roots() -> TrustRoots {
    APP_TRUST_ROOTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_else(TrustRoots::public_roots)
}

/// The app's set: without `std` there is no app-wide setting, so the public roots.
#[cfg(not(feature = "std"))]
#[must_use]
pub fn app_trust_roots() -> TrustRoots {
    TrustRoots::public_roots()
}

#[cfg(all(feature = "tls-roots", not(target_arch = "wasm32")))]
pub use self::roots::{pem_certificate, RootCertificate};

/// The trusted roots as certificates, the one form every TLS library here takes (rustls'
/// root store, ureq's, iroh's, reqwest's, lettre's).
#[cfg(all(feature = "tls-roots", not(target_arch = "wasm32")))]
mod roots {
    use alloc::{format, string::String, vec::Vec};

    use rustls_pki_types::{pem::PemObject, CertificateDer};

    use super::{TrustRootSet, TrustRoots};

    /// One trusted root certificate, DER.
    pub type RootCertificate = CertificateDer<'static>;

    impl TrustRoots {
        /// The root certificates a connection made now trusts: the resolved set
        /// ([`TrustRoots::resolved`]) without its distrusted roots. Every TLS client of azul
        /// and its apps builds its root store from this list, so a set means the same
        /// everywhere.
        ///
        /// # Errors
        ///
        /// A pinned or distrusted entry that does not parse or holds no certificate, named by
        /// its place in the list. A connection then fails; it never falls back to the public
        /// roots.
        pub fn certificates(&self) -> Result<Vec<RootCertificate>, String> {
            self.resolved().certificates_from(webpki_root_certs::TLS_SERVER_ROOT_CERTS)
        }

        /// [`TrustRoots::certificates`] with `public` as the public roots: the tests stand in
        /// their own authorities for Mozilla's, which no test server can be issued under.
        ///
        /// # Errors
        ///
        /// As [`TrustRoots::certificates`].
        pub fn certificates_from(
            &self,
            public: &[RootCertificate],
        ) -> Result<Vec<RootCertificate>, String> {
            let resolved = self.resolved();
            let mut set = match &resolved.roots {
                TrustRootSet::AppDefault | TrustRootSet::PublicRoots => public.to_vec(),
                TrustRootSet::Pinned(pems) => parse("trusted root", pems)?,
            };
            if resolved.distrusted.as_ref().is_empty() {
                return Ok(set);
            }
            let distrusted = parse("distrusted root", &resolved.distrusted)?
                .iter()
                .enumerate()
                .map(|(i, cert)| {
                    public_key(cert)
                        .ok_or_else(|| format!("distrusted root {i} is not a certificate"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            // A root whose key cannot be read is kept: no TLS library could trust it either.
            set.retain(|cert| public_key(cert).is_none_or(|key| !distrusted.contains(&key)));
            Ok(set)
        }
    }

    /// The certificates of `pems`, each entry at least one.
    fn parse(what: &str, pems: &azul_css::StringVec) -> Result<Vec<RootCertificate>, String> {
        let mut certs = Vec::new();
        for (i, pem) in pems.as_ref().iter().enumerate() {
            let before = certs.len();
            for cert in CertificateDer::pem_slice_iter(pem.as_str().as_bytes()) {
                certs.push(cert.map_err(|e| format!("{what} {i} does not parse: {e:?}"))?);
            }
            if certs.len() == before {
                return Err(format!("{what} {i} holds no certificate"));
            }
        }
        Ok(certs)
    }

    /// `der` as one PEM `CERTIFICATE` block.
    #[must_use]
    pub fn pem_certificate(der: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut text = String::with_capacity(der.len() * 4 / 3 + 4);
        for chunk in der.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..4 {
                text.push(if i <= chunk.len() {
                    char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize])
                } else {
                    '='
                });
            }
        }
        let mut out = String::from("-----BEGIN CERTIFICATE-----\n");
        for line in text.as_bytes().chunks(64) {
            out.push_str(core::str::from_utf8(line).unwrap_or_default());
            out.push('\n');
        }
        out.push_str("-----END CERTIFICATE-----\n");
        out
    }

    /// The `SubjectPublicKeyInfo` of a root certificate: what a distrusted root is matched by.
    fn public_key(cert: &RootCertificate) -> Option<Vec<u8>> {
        webpki::anchor_from_trusted_cert(cert)
            .ok()
            .map(|anchor| anchor.subject_public_key_info.as_ref().to_vec())
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
