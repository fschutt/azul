#[cfg(feature = "alloc")]
use alloc::{boxed::Box, format, string::ToString, sync::Arc};

use pkcs8::DecodePrivateKey;
use pki_types::PrivateKeyDer;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::RsaPrivateKey;
use rustls::sign::{Signer, SigningKey};
use rustls::{SignatureAlgorithm, SignatureScheme};
use sha2::{Sha256, Sha384, Sha512};

const ALL_RSA_SCHEMES: &[SignatureScheme] = &[
    SignatureScheme::RSA_PSS_SHA512,
    SignatureScheme::RSA_PSS_SHA384,
    SignatureScheme::RSA_PSS_SHA256,
    SignatureScheme::RSA_PKCS1_SHA512,
    SignatureScheme::RSA_PKCS1_SHA384,
    SignatureScheme::RSA_PKCS1_SHA256,
];

#[derive(Debug)]
pub struct RsaSigningKey(RsaPrivateKey);

impl TryFrom<&PrivateKeyDer<'_>> for RsaSigningKey {
    type Error = rustls::Error;

    fn try_from(value: &PrivateKeyDer<'_>) -> Result<Self, Self::Error> {
        let pkey = match value {
            PrivateKeyDer::Pkcs8(der) => RsaPrivateKey::from_pkcs8_der(der.secret_pkcs8_der())
                .map_err(|e| format!("failed to decrypt private key: {e}")),
            PrivateKeyDer::Pkcs1(der) => RsaPrivateKey::from_pkcs1_der(der.secret_pkcs1_der())
                .map_err(|e| format!("failed to decrypt private key: {e}")),
            PrivateKeyDer::Sec1(_) => Err("RSA does not support SEC-1 key".to_string()),
            _ => Err("not supported".into()),
        };

        pkey.map(Self).map_err(rustls::Error::General)
    }
}

impl SigningKey for RsaSigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        ALL_RSA_SCHEMES
            .iter()
            .find(|scheme| offered.contains(scheme))
            .and_then(|&scheme| {
                macro_rules! signer {
                    ($key:ty) => {{
                        Some(Box::new(super::GenericRandomizedSigner {
                            _marker: Default::default(),
                            key: Arc::new(<$key>::new(self.0.clone())),
                            scheme,
                        }) as Box<_>)
                    }};
                }

                match scheme {
                    SignatureScheme::RSA_PSS_SHA512 => signer! {rsa::pss::SigningKey::<Sha512>},
                    SignatureScheme::RSA_PSS_SHA384 => signer! {rsa::pss::SigningKey::<Sha384>},
                    SignatureScheme::RSA_PSS_SHA256 => signer! {rsa::pss::SigningKey::<Sha256>},
                    SignatureScheme::RSA_PKCS1_SHA512 => {
                        signer! {rsa::pkcs1v15::SigningKey::<Sha512>}
                    }
                    SignatureScheme::RSA_PKCS1_SHA384 => {
                        signer! {rsa::pkcs1v15::SigningKey::<Sha384>}
                    }
                    SignatureScheme::RSA_PKCS1_SHA256 => {
                        signer! {rsa::pkcs1v15::SigningKey::<Sha256>}
                    }
                    _ => None,
                }
            })
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::RSA
    }
}
