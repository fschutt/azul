#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use super::{DecryptBufferAdapter, EncryptBufferAdapter};

use aead::AeadInPlace;
use crypto_common::{KeyInit, KeySizeUser};
use paste::paste;
use rustls::crypto::cipher::{
    self, AeadKey, InboundOpaqueMessage, InboundPlainMessage, MessageDecrypter, MessageEncrypter,
    OutboundOpaqueMessage, OutboundPlainMessage, PrefixedPayload, Tls13AeadAlgorithm,
};
use rustls::{ConnectionTrafficSecrets, ContentType, ProtocolVersion};

#[cfg(feature = "tls12")]
use {
    aead::AeadCore,
    crypto_common::typenum::Unsigned,
    rustls::crypto::cipher::{Iv, KeyBlockShape, Tls12AeadAlgorithm},
};

#[cfg(feature = "tls12")]
const TLS12_GCM_EXPLICIT_NONCE_LEN: usize = 8;

#[cfg(feature = "tls12")]
const TLS12_GCM_OVERHEAD: usize = TLS12_GCM_EXPLICIT_NONCE_LEN + 16;

macro_rules! impl_gcm_tls13 {
    ($name: ident, $aead: ty, $overhead: expr) => {
        paste! {
            pub struct [<Tls13 $name>];

            impl Tls13AeadAlgorithm for [<Tls13 $name>] {
                fn encrypter(&self, key: AeadKey, iv: cipher::Iv) -> Box<dyn MessageEncrypter> {
                    Box::new([<Tls13Cipher $name>](
                        $aead::new_from_slice(key.as_ref()).unwrap(),
                        iv,
                    ))
                }

                fn decrypter(&self, key: AeadKey, iv: cipher::Iv) -> Box<dyn MessageDecrypter> {
                    Box::new([<Tls13Cipher $name>](
                        $aead::new_from_slice(key.as_ref()).unwrap(),
                        iv,
                    ))
                }

                fn key_len(&self) -> usize {
                    $aead::key_size()
                }
                fn extract_keys(
                    &self,
                    key: AeadKey,
                    iv: cipher::Iv,
                ) -> Result<ConnectionTrafficSecrets, cipher::UnsupportedOperationError> {
                    Ok(ConnectionTrafficSecrets::Aes256Gcm { key, iv })
                }
            }

            struct [<Tls13Cipher $name>]($aead, cipher::Iv);

            impl MessageEncrypter for [<Tls13Cipher $name>] {
                fn encrypt(&mut self, m: OutboundPlainMessage<'_>, seq: u64) -> Result<OutboundOpaqueMessage, rustls::Error> {
                    let total_len = self.encrypted_payload_len(m.payload.len());
                    let mut payload = PrefixedPayload::with_capacity(total_len);

                    let nonce = cipher::Nonce::new(&self.1, seq).0;
                    let aad = cipher::make_tls13_aad(total_len);
                    payload.extend_from_chunks(&m.payload);
                    payload.extend_from_slice(&m.typ.to_array());

                    self.0
                        .encrypt_in_place(&nonce.into(), &aad, &mut EncryptBufferAdapter(&mut payload))
                        .map_err(|_| rustls::Error::EncryptError)
                        .map(|_| OutboundOpaqueMessage::new(
                            ContentType::ApplicationData,
                            ProtocolVersion::TLSv1_2,
                            payload,
                        ))
                }

                fn encrypted_payload_len(&self, payload_len: usize) -> usize {
                    payload_len + 1 + $overhead
                }
            }

            impl MessageDecrypter for [<Tls13Cipher $name>] {
                fn decrypt<'a>(&mut self, mut m: InboundOpaqueMessage<'a>, seq: u64) -> Result<InboundPlainMessage<'a>, rustls::Error> {
                    let payload = &mut m.payload;
                    let nonce = cipher::Nonce::new(&self.1, seq).0;
                    let aad = cipher::make_tls13_aad(payload.len());

                    self.0
                        .decrypt_in_place(&nonce.into(), &aad, &mut DecryptBufferAdapter(payload))
                        .map_err(|_| rustls::Error::DecryptError)?;

                    m.into_tls13_unpadded_message()
                }
            }

        }
    };
}

#[cfg(feature = "tls12")]
macro_rules! impl_gcm_tls12 {
    ($name: ident, $aead: ty, $nonce: expr, $overhead: expr) => {
        paste! {
            #[cfg(feature = "tls12")]
            pub struct [<Tls12 $name>];

            #[cfg(feature = "tls12")]
            impl Tls12AeadAlgorithm for [<Tls12 $name>] {
                fn encrypter(&self, key: AeadKey, write_iv: &[u8], explicit: &[u8]) -> Box<dyn MessageEncrypter> {
                    Box::new([<Tls12Cipher $name Encrypter>](
                        $aead::new_from_slice(key.as_ref()).unwrap(),
                        {
                            let mut iv: [u8; 12] = [0; 12];
                            iv[..4].copy_from_slice(write_iv);
                            iv[4..].copy_from_slice(explicit);
                            iv
                        },
                    ))
                }

                fn decrypter(&self, dec_key: AeadKey, dec_iv: &[u8]) -> Box<dyn MessageDecrypter> {
                    Box::new([<Tls12Cipher $name Decrypter>](
                        $aead::new_from_slice(dec_key.as_ref()).unwrap(),
                        dec_iv.try_into().unwrap(),
                    ))
                }

                fn key_block_shape(&self) -> KeyBlockShape {
                    KeyBlockShape {
                        enc_key_len:        $aead::key_size(),
                        fixed_iv_len:       4,
                        explicit_nonce_len: 8,
                    }
                }

                fn extract_keys(
                    &self,
                    key: AeadKey,
                    iv: &[u8],
                    _explicit: &[u8],
                ) -> Result<ConnectionTrafficSecrets, cipher::UnsupportedOperationError> {
                    Ok(ConnectionTrafficSecrets::Aes128Gcm {
                        key,
                        iv: Iv::new(iv[..].try_into().unwrap()),
                    })
                }
            }

            #[cfg(feature = "tls12")]
            struct [<Tls12Cipher $name Encrypter>]($aead, [u8; 12]);

            #[cfg(feature = "tls12")]
            impl MessageEncrypter for [<Tls12Cipher $name Encrypter>] {
                fn encrypt(&mut self, m: OutboundPlainMessage<'_>, seq: u64) -> Result<OutboundOpaqueMessage, rustls::Error> {
                    let total_len = self.encrypted_payload_len(m.payload.len());
                    let mut payload = PrefixedPayload::with_capacity(total_len);

                    let nonce = cipher::Nonce::new(&self.1.into(), seq).0;
                    let aad = cipher::make_tls12_aad(seq, m.typ, m.version, m.payload.len());
                    payload.extend_from_slice(&nonce.as_ref()[4..]); // explicit
                    payload.extend_from_chunks(&m.payload);

                    self.0
                        .encrypt_in_place_detached(&nonce.into(), &aad, &mut payload.as_mut()[$nonce..])
                        .map(|tag| payload.extend(tag.as_ref() as &[u8]))
                        .map_err(|_| rustls::Error::EncryptError)
                        .map(|_| OutboundOpaqueMessage::new(m.typ, m.version, payload))
                }
                fn encrypted_payload_len(&self, payload_len: usize) -> usize {
                    payload_len + $nonce + <$aead as AeadCore>::TagSize::USIZE
                }
            }

            #[cfg(feature = "tls12")]
            struct [<Tls12Cipher $name Decrypter>]($aead, [u8; 4]);

            #[cfg(feature = "tls12")]
            impl MessageDecrypter for [<Tls12Cipher $name Decrypter>] {
                fn decrypt<'a>(&mut self, mut m: InboundOpaqueMessage<'a>, seq: u64) -> Result<InboundPlainMessage<'a>, rustls::Error> {
                    type TagSize = <$aead as AeadCore>::TagSize;

                    let payload = &m.payload;

                    if payload.len() < $overhead {
                        return Err(rustls::Error::DecryptError);
                    }

                    let nonce: aead::Nonce<$aead> = {
                        let mut nonce = [0u8; 12];
                        nonce[..4].copy_from_slice(&self.1); // dec_iv
                        nonce[4..].copy_from_slice(&payload[..$nonce]);
                        nonce.into()
                    };

                    let aad = cipher::make_tls12_aad(seq, m.typ, m.version, payload.len() - $overhead);

                    let payload = &mut m.payload;
                    let tag_pos = {
                        let payload = &mut payload[$nonce..];
                        let tag_pos = payload.len() - TagSize::to_usize();
                        let (msg, tag) = payload.split_at_mut(tag_pos);

                        let tag = aes_gcm::Tag::<TagSize>::from_slice(tag);
                        self.0
                            .decrypt_in_place_detached(&nonce, &aad, msg, tag)
                            .map_err(|_| rustls::Error::DecryptError)?;
                        tag_pos
                    };

                    // We defer the truncation to here, because we may inadvertently shifted the
                    // original data if the decryption failed. Another way to avoid this is
                    // to clone the payload slice starting after the explicit nonce,
                    // but this will cause an additional cloning and copying
                    payload.rotate_left($nonce);
                    payload.truncate(tag_pos);
                    Ok(m.into_plain_message())
                }
            }
        }
    };
}

impl_gcm_tls13! {Aes128Gcm, aes_gcm::Aes128Gcm, 16}
impl_gcm_tls13! {Aes256Gcm, aes_gcm::Aes256Gcm, 16}

#[cfg(feature = "tls12")]
impl_gcm_tls12! {Aes128Gcm, aes_gcm::Aes128Gcm, TLS12_GCM_EXPLICIT_NONCE_LEN, TLS12_GCM_OVERHEAD}

#[cfg(feature = "tls12")]
impl_gcm_tls12! {Aes256Gcm, aes_gcm::Aes256Gcm, TLS12_GCM_EXPLICIT_NONCE_LEN, TLS12_GCM_OVERHEAD}
