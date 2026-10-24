#![no_std]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/RustCrypto/meta/master/logo.svg",
    html_favicon_url = "https://raw.githubusercontent.com/RustCrypto/meta/master/logo.svg"
)]
#![warn(
    clippy::alloc_instead_of_core,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::checked_conversions,
    clippy::from_iter_instead_of_collect,
    clippy::missing_errors_doc,
    clippy::mod_module_files,
    clippy::implicit_saturating_sub,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    clippy::unwrap_used,
    rust_2018_idioms,
    trivial_numeric_casts,
    unused_lifetimes
)]

//! # Usage
//!
//! See [`examples-xsmall`](https://github.com/RustCrypto/rustls-rustcrypto/tree/master/examples-xsmall)
//! for a usage example.

#[cfg(not(feature = "alloc"))]
compile_error!("Rustls currently does not support alloc-less environments");

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::sync::Arc;

use rustls::crypto::{
    CipherSuiteCommon, CryptoProvider, GetRandomFailed, KeyProvider, SecureRandom,
};
use rustls::{CipherSuite, SupportedCipherSuite, Tls13CipherSuite};

#[cfg(feature = "tls12")]
use rustls::SignatureScheme;

#[derive(Debug)]
pub struct Provider;

pub fn provider() -> CryptoProvider {
    CryptoProvider {
        cipher_suites: ALL_CIPHER_SUITES.to_vec(),
        kx_groups: kx::ALL_KX_GROUPS.to_vec(),
        signature_verification_algorithms: verify::ALGORITHMS,
        secure_random: &Provider,
        key_provider: &Provider,
    }
}

impl SecureRandom for Provider {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), GetRandomFailed> {
        use rand_core::RngCore;
        rand_core::OsRng
            .try_fill_bytes(bytes)
            .map_err(|_| GetRandomFailed)
    }
}

impl KeyProvider for Provider {
    fn load_private_key(
        &self,
        key_der: pki_types::PrivateKeyDer<'static>,
    ) -> Result<Arc<dyn rustls::sign::SigningKey>, rustls::Error> {
        sign::any_supported_type(&key_der)
    }
}

#[cfg(feature = "tls12")]
const TLS12_ECDSA_SCHEMES: [SignatureScheme; 4] = [
    SignatureScheme::ECDSA_NISTP256_SHA256,
    SignatureScheme::ECDSA_NISTP384_SHA384,
    SignatureScheme::ECDSA_NISTP521_SHA512,
    SignatureScheme::ED25519,
];

#[cfg(feature = "tls12")]
const TLS12_RSA_SCHEMES: [SignatureScheme; 6] = [
    SignatureScheme::RSA_PKCS1_SHA256,
    SignatureScheme::RSA_PKCS1_SHA384,
    SignatureScheme::RSA_PKCS1_SHA512,
    SignatureScheme::RSA_PSS_SHA256,
    SignatureScheme::RSA_PSS_SHA384,
    SignatureScheme::RSA_PSS_SHA512,
];

#[cfg(feature = "tls12")]
pub const TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&rustls::Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: rustls::crypto::KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_ECDSA_SCHEMES,
        aead_alg: &aead::gcm::Tls12Aes128Gcm,
        prf_provider: &rustls::crypto::tls12::PrfUsingHmac(hmac::SHA256),
    });

#[cfg(feature = "tls12")]
pub const TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&rustls::Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
            hash_provider: hash::SHA384,
            confidentiality_limit: u64::MAX,
        },
        kx: rustls::crypto::KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_ECDSA_SCHEMES,
        prf_provider: &rustls::crypto::tls12::PrfUsingHmac(hmac::SHA384),
        aead_alg: &aead::gcm::Tls12Aes256Gcm,
    });

#[cfg(feature = "tls12")]
pub const TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&rustls::Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        prf_provider: &rustls::crypto::tls12::PrfUsingHmac(hmac::SHA256),
        kx: rustls::crypto::KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_ECDSA_SCHEMES,
        aead_alg: &aead::chacha20::Chacha20Poly1305,
    });

#[cfg(feature = "tls12")]
const TLS_ECDHE_ECDSA_SUITES: &[SupportedCipherSuite] = &[
    TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
    TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
    TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
];

#[cfg(feature = "tls12")]
pub const TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&rustls::Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: rustls::crypto::KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_RSA_SCHEMES,
        aead_alg: &aead::gcm::Tls12Aes128Gcm,
        prf_provider: &rustls::crypto::tls12::PrfUsingHmac(hmac::SHA256),
    });

#[cfg(feature = "tls12")]
pub const TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&rustls::Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            hash_provider: hash::SHA384,
            confidentiality_limit: u64::MAX,
        },
        kx: rustls::crypto::KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_RSA_SCHEMES,
        prf_provider: &rustls::crypto::tls12::PrfUsingHmac(hmac::SHA384),
        aead_alg: &aead::gcm::Tls12Aes256Gcm,
    });

#[cfg(feature = "tls12")]
pub const TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&rustls::Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: rustls::crypto::KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_RSA_SCHEMES,
        prf_provider: &rustls::crypto::tls12::PrfUsingHmac(hmac::SHA256),
        aead_alg: &aead::chacha20::Chacha20Poly1305,
    });

#[cfg(feature = "tls12")]
const TLS_ECDHE_RSA_SUITES: &[SupportedCipherSuite] = &[
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
    TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
    TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
];

#[cfg(feature = "tls12")]
const TLS12_SUITES: &[SupportedCipherSuite] = misc::const_concat_slices!(
    SupportedCipherSuite,
    TLS_ECDHE_ECDSA_SUITES,
    TLS_ECDHE_RSA_SUITES
);

#[cfg(not(feature = "tls12"))]
const TLS12_SUITES: &[SupportedCipherSuite] = &[];

pub const TLS13_AES_128_GCM_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls13(&Tls13CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS13_AES_128_GCM_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        hkdf_provider: &rustls::crypto::tls13::HkdfUsingHmac(hmac::SHA256),
        aead_alg: &aead::gcm::Tls13Aes128Gcm,
        quic: None,
    });

pub const TLS13_AES_256_GCM_SHA384: SupportedCipherSuite =
    SupportedCipherSuite::Tls13(&Tls13CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS13_AES_256_GCM_SHA384,
            hash_provider: hash::SHA384,
            confidentiality_limit: u64::MAX,
        },
        hkdf_provider: &rustls::crypto::tls13::HkdfUsingHmac(hmac::SHA384),
        aead_alg: &aead::gcm::Tls13Aes256Gcm,
        quic: None,
    });

const TLS13_AES_SUITES: &[SupportedCipherSuite] =
    &[TLS13_AES_128_GCM_SHA256, TLS13_AES_256_GCM_SHA384];

pub const TLS13_CHACHA20_POLY1305_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls13(&Tls13CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        hkdf_provider: &rustls::crypto::tls13::HkdfUsingHmac(hmac::SHA256),
        aead_alg: &aead::chacha20::Chacha20Poly1305,
        quic: None,
    });

const TLS13_SUITES: &[SupportedCipherSuite] = misc::const_concat_slices!(
    SupportedCipherSuite,
    TLS13_AES_SUITES,
    &[TLS13_CHACHA20_POLY1305_SHA256]
);

static ALL_CIPHER_SUITES: &[SupportedCipherSuite] = misc::const_concat_slices!(
    SupportedCipherSuite,
    if cfg!(feature = "tls12") {
        TLS12_SUITES
    } else {
        &[]
    },
    TLS13_SUITES,
);

mod aead;
mod hash;
mod hmac;
mod kx;
mod misc;
pub mod quic;
pub mod sign;
mod verify;
