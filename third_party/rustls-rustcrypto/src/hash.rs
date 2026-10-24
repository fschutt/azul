#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use digest::{Digest, OutputSizeUser};
use paste::paste;
use rustls::crypto::{self, hash};
use sha2::{Sha256, Sha384};

macro_rules! impl_hash {
    ($name:ident, $ty:ty, $algo:ty) => {
        paste! {
            #[allow(non_camel_case_types)]
            struct [<Hash_ $ty>];

            impl hash::Hash for [<Hash_ $ty>] {
                fn start(&self) -> Box<dyn hash::Context> {
                    Box::new([<HashContent_ $ty>]($ty::new()))
                }

                fn hash(&self, data: &[u8]) -> hash::Output {
                    hash::Output::new(&$ty::digest(data)[..])
                }

                fn output_len(&self) -> usize {
                    <$ty as OutputSizeUser>::output_size()
                }

                fn algorithm(&self) -> hash::HashAlgorithm {
                    $algo
                }
            }

            #[allow(non_camel_case_types)]
            struct [<HashContent_ $ty>]($ty);

            impl hash::Context for [<HashContent_ $ty>] {
                fn fork_finish(&self) -> hash::Output {
                    hash::Output::new(&self.0.clone().finalize()[..])
                }

                fn fork(&self) -> Box<dyn hash::Context> {
                    Box::new([<HashContent_ $ty>](self.0.clone()))
                }

                fn finish(self: Box<Self>) -> hash::Output {
                    hash::Output::new(&self.0.finalize()[..])
                }

                fn update(&mut self, data: &[u8]) {
                    self.0.update(data);
                }
            }

            pub const $name: &dyn crypto::hash::Hash = &[<Hash_ $ty>];
        }
    };
}

// impl_hash! {SHA224, Sha224, hash::HashAlgorithm::SHA224}
impl_hash! {SHA256, Sha256, hash::HashAlgorithm::SHA256}
impl_hash! {SHA384, Sha384, hash::HashAlgorithm::SHA384}
// impl_hash! {SHA512, Sha512, hash::HashAlgorithm::SHA512}
