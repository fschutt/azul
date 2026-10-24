use der::Decode;
use digest::Digest;
use paste::paste;
use pki_types::{AlgorithmIdentifier, InvalidSignature, SignatureVerificationAlgorithm};
use signature::hazmat::PrehashVerifier;
use pki_types::alg_id;

macro_rules! impl_generic_ecdsa_verifer {
    (
        $name:ident,
        $public_key_algo:expr,
        $signature_alg_id:expr,
        $verifying_key:ty,
        $signature:ty,
        $hash:ty
    ) => {
        paste! {
            #[allow(non_camel_case_types)]
            #[derive(Debug)]
            struct [<EcdsaVerifier_ $name>];

            impl SignatureVerificationAlgorithm for [<EcdsaVerifier_ $name>] {
                fn public_key_alg_id(&self) -> AlgorithmIdentifier {
                    $public_key_algo
                }

                fn signature_alg_id(&self) -> AlgorithmIdentifier {
                    $signature_alg_id
                }

                fn verify_signature(
                    &self,
                    public_key: &[u8],
                    message: &[u8],
                    signature: &[u8],
                ) -> Result<(), InvalidSignature> {
                    let signature = <$signature>::from_der(signature).map_err(|_| InvalidSignature)?;
                    let verifying_key = <$verifying_key>::from_sec1_bytes(public_key).map_err(|_| InvalidSignature)?;
                    let digest = &<$hash>::digest(&message);
                    verifying_key
                        .verify_prehash(digest, &signature)
                        .map_err(|_| InvalidSignature)
                }
            }

            pub const $name: &dyn SignatureVerificationAlgorithm = &[<EcdsaVerifier_ $name>];
        }
    };
}

impl_generic_ecdsa_verifer! {ECDSA_P256_SHA256, alg_id::ECDSA_P256, alg_id::ECDSA_SHA256, p256::ecdsa::VerifyingKey, p256::ecdsa::DerSignature, sha2::Sha256}
impl_generic_ecdsa_verifer! {ECDSA_P256_SHA384, alg_id::ECDSA_P256, alg_id::ECDSA_SHA384, p256::ecdsa::VerifyingKey, p256::ecdsa::DerSignature, sha2::Sha384}
impl_generic_ecdsa_verifer! {ECDSA_P384_SHA256, alg_id::ECDSA_P384, alg_id::ECDSA_SHA256, p384::ecdsa::VerifyingKey, p384::ecdsa::DerSignature, sha2::Sha256}
impl_generic_ecdsa_verifer! {ECDSA_P384_SHA384, alg_id::ECDSA_P384, alg_id::ECDSA_SHA384, p384::ecdsa::VerifyingKey, p384::ecdsa::DerSignature, sha2::Sha384}
