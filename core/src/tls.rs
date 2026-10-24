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
