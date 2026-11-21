//! A small secret file sealed under a key the device's keyring holds (AZ-K13): the period
//! vault's issue keys and tokens, which are bearers for months a customer paid for.
//!
//! The file is `AZB1`, a random 192-bit nonce and XChaCha20-Poly1305's ciphertext; the associated
//! data is a label and the CONTEXT the caller names (the drive and the file's role), so a sealed
//! file moved to another drive's name, or to the second copy's, does not open. The key is a
//! random 256 bits that lives in the keyring and nowhere else - never beside the file; whoever
//! can read the file alone reads ciphertext. No primitive is made here (`chacha20poly1305`).

use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};

use super::{random_bytes, CryptoError, Zeroizing};

/// The key's length.
pub const KEY_LEN: usize = 32;

/// What a sealed file starts with. JSON starts with `{`, so a plain file is never mistaken for a
/// sealed one.
const MAGIC: &[u8; 4] = b"AZB1";
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const AAD_LABEL: &[u8] = b"azul-storage/blob/v1|";

/// Whether `bytes` is a sealed file (it starts with the magic; whether it opens is [`open`]'s).
#[must_use]
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

fn aad(context: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(AAD_LABEL.len() + context.len());
    aad.extend_from_slice(AAD_LABEL);
    aad.extend_from_slice(context.as_bytes());
    aad
}

/// A fresh random key.
///
/// # Errors
///
/// [`CryptoError::Random`] when the OS random source fails.
pub fn new_key() -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    random_bytes(key.as_mut())?;
    Ok(key)
}

/// `plain` sealed under `key` for `context`, with a fresh random nonce.
///
/// # Errors
///
/// [`CryptoError::Random`] or [`CryptoError::Damaged`] when the cipher refuses.
pub fn seal(key: &[u8; KEY_LEN], context: &str, plain: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce)?;
    let sealed = XChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: &aad(context),
            },
        )
        .map_err(|_| CryptoError::Damaged(String::from("the cipher refused to seal a file")))?;
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// The plain bytes of `sealed`, opened with `key` for `context`.
///
/// # Errors
///
/// [`CryptoError::Damaged`] when `sealed` is no sealed file, is cut short, was changed, belongs to
/// another context, or another key sealed it (a failed authentication cannot tell which).
pub fn open(
    key: &[u8; KEY_LEN],
    context: &str,
    sealed: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let body = sealed
        .strip_prefix(MAGIC.as_slice())
        .ok_or_else(|| CryptoError::Damaged(String::from("it is no sealed file")))?;
    if body.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::Damaged(String::from(
            "the sealed file is cut short",
        )));
    }
    let (nonce, ciphertext) = body.split_at(NONCE_LEN);
    XChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad(context),
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| {
            CryptoError::Damaged(String::from(
                "the sealed file does not authenticate for this key and place",
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: &str = "azcloud-kit/period-vault|d_1";

    #[test]
    fn a_sealed_file_opens_with_its_key_and_its_place_and_only_then() {
        let key = new_key().unwrap();
        let plain = br#"{"grants":[{"issue_key":"SECRET"}]}"#;
        let sealed = seal(&key, CONTEXT, plain).unwrap();
        assert!(is_sealed(&sealed));
        assert!(
            !sealed.windows(6).any(|w| w == b"SECRET"),
            "the secret is in the clear"
        );
        assert_eq!(open(&key, CONTEXT, &sealed).unwrap().as_slice(), plain);

        let other = new_key().unwrap();
        assert!(open(&other, CONTEXT, &sealed).is_err(), "another key");
        assert!(
            open(&key, "azcloud-kit/period-vault|d_2", &sealed).is_err(),
            "another drive's name"
        );
        assert!(
            open(&key, "azcloud-kit/period-vault|d_1.copy", &sealed).is_err(),
            "the second copy's place"
        );
    }

    #[test]
    fn a_changed_or_cut_file_does_not_open() {
        let key = new_key().unwrap();
        let sealed = seal(&key, CONTEXT, b"{}").unwrap();
        for at in [4, 10, 28, sealed.len() - 1] {
            let mut bad = sealed.clone();
            bad[at] ^= 1;
            assert!(open(&key, CONTEXT, &bad).is_err(), "byte {at}");
        }
        for len in [0, 3, 4, 20, 27, 43] {
            assert!(
                open(&key, CONTEXT, &sealed[..len.min(sealed.len())]).is_err(),
                "{len}"
            );
        }
        assert!(
            open(&key, CONTEXT, br#"{"grants":[]}"#).is_err(),
            "plain JSON"
        );
    }

    #[test]
    fn a_plain_json_file_is_never_taken_for_a_sealed_one() {
        assert!(!is_sealed(br#"{"format":2}"#));
        assert!(!is_sealed(b""));
        assert!(!is_sealed(b"AZB"));
        assert!(is_sealed(b"AZB1\x00"));
    }

    #[test]
    fn the_same_text_seals_differently_every_time_and_keys_differ() {
        let key = new_key().unwrap();
        let a = seal(&key, CONTEXT, b"same").unwrap();
        let b = seal(&key, CONTEXT, b"same").unwrap();
        assert_ne!(a, b, "a nonce is reused");
        assert_ne!(new_key().unwrap().as_ref(), new_key().unwrap().as_ref());
    }
}
