//! What the mail connections trust: the IMAP sync's TLS, STARTTLS of direct delivery and of a
//! relay, signed-in submission.
//!
//! The host hands in the roots its app trusts as PEM text, one certificate each: AzMail the
//! app's set (`TrustRoots::get_certificates_pem`, the public roots unless the user pinned or
//! distrusted some), the Azlin Bridge the set of its config. Without them the Mozilla roots
//! apply. An account's extra certificate (a test server's) is trusted on top of either.

use std::{path::Path, sync::Arc};

use rustls::pki_types::{pem::PemObject, CertificateDer};

/// The roots the mail connections trust, PEM text (each entry one or more certificates):
/// exactly these, nothing of the Mozilla set. `None`: the Mozilla roots.
pub type TrustedRoots = Option<Vec<String>>;

/// The certificates of `pems`, each entry at least one; an entry that does not parse is an
/// error naming its place. A connection then fails: it never falls back to the Mozilla roots.
///
/// # Errors
///
/// As above.
pub fn certificates(pems: &[String]) -> Result<Vec<CertificateDer<'static>>, String> {
    let mut certs = Vec::new();
    for (i, pem) in pems.iter().enumerate() {
        let before = certs.len();
        for cert in CertificateDer::pem_slice_iter(pem.as_bytes()) {
            certs.push(cert.map_err(|e| format!("trusted root {i} does not parse: {e:?}"))?);
        }
        if certs.len() == before {
            return Err(format!("trusted root {i} holds no certificate"));
        }
    }
    Ok(certs)
}

/// The root store of `roots` (the Mozilla roots for `None`), with `extra_ca` on top.
///
/// # Errors
///
/// A root or the extra certificate that does not parse.
pub fn root_store(
    roots: Option<&[String]>,
    extra_ca: Option<&Path>,
) -> Result<rustls::RootCertStore, String> {
    let mut store = rustls::RootCertStore::empty();
    match roots {
        Some(pems) => {
            for cert in certificates(pems)? {
                store.add(cert).map_err(|e| format!("trusted root: {e}"))?;
            }
        }
        None => store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
    }
    if let Some(path) = extra_ca {
        let cert = CertificateDer::from_pem_file(path)
            .map_err(|e| format!("the certificate file {}: {e}", path.display()))?;
        store
            .add(cert)
            .map_err(|e| format!("the certificate file {}: {e}", path.display()))?;
    }
    Ok(store)
}

/// A rustls client configuration trusting [`root_store`], with the pure-Rust crypto provider
/// (the one azul's HTTP client uses): what the IMAP sync and micromail's STARTTLS take.
///
/// # Errors
///
/// As [`root_store`].
pub fn client_config(
    roots: Option<&[String]>,
    extra_ca: Option<&Path>,
) -> Result<Arc<rustls::ClientConfig>, String> {
    let store = root_store(roots, extra_ca)?;
    let config =
        rustls::ClientConfig::builder_with_provider(Arc::new(rustls_rustcrypto::provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(store)
            .with_no_client_auth();
    Ok(Arc::new(config))
}
