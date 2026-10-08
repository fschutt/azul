//! A room's invite secret and the invite key it derives (CRYPTO.md section 4), and nothing else:
//! the part of AzMeet's crypto an app that makes meeting links of its own needs. AzCalendar
//! includes this file (`#[path]`, as it includes `rooms.rs`): it mints a room id and an invite
//! secret for an event, registers the room with the invite key, and keeps the link with the
//! secret as its fragment, so the meeting is end-to-end encrypted like one AzMeet made.
//! `crypto::Invite` derives its signing key here, so the two never differ. Pure: hkdf, sha2 and
//! ed25519-dalek, no azul types.

use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use sha2::Sha256;

/// The HKDF salt of everything derived from an invite secret.
pub const INVITE_SALT: &[u8] = b"azmeet/v1/invite";
/// The alphabet of room ids and invite secrets (lower-case Crockford base32).
pub const SECRET_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
/// The characters of an invite secret (130 random bits).
pub const SECRET_LEN: usize = 26;

/// Whether `secret` is an invite secret: 26 characters of the alphabet.
#[must_use]
pub fn is_secret(secret: &str) -> bool {
    secret.len() == SECRET_LEN && secret.bytes().all(|b| SECRET_ALPHABET.contains(&b))
}

/// The seed of the invite's signing key for room `room`: HKDF-SHA256 of the secret, salt
/// [`INVITE_SALT`], info `sign\n<room>`. The caller wipes it.
#[must_use]
pub fn sign_seed(room: &str, secret: &str) -> [u8; 32] {
    let mut seed = [0u8; 32];
    Hkdf::<Sha256>::new(Some(INVITE_SALT), secret.as_bytes())
        .expand(format!("sign\n{room}").as_bytes(), &mut seed)
        .expect("HKDF-SHA256 gives 32 bytes");
    seed
}

/// The invite key (the Ed25519 public key, hex) the room `room` is registered with when its link
/// carries `secret`; `None` for a secret that is none.
#[must_use]
pub fn invite_key(room: &str, secret: &str) -> Option<String> {
    if !is_secret(secret) {
        return None;
    }
    let mut seed = sign_seed(room, secret);
    let key = SigningKey::from_bytes(&seed);
    seed.fill(0);
    Some(
        key.verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// 26 characters of the alphabet from 130 of `entropy`'s bits (both first words, the two lowest
/// bits of the third): an invite secret, or a room id, from three random 64-bit words.
#[must_use]
pub fn secret_from(entropy: [u64; 3]) -> String {
    let mut bits = (u128::from(entropy[0]) << 64) | u128::from(entropy[1]);
    (0..SECRET_LEN)
        .map(|i| {
            let value = if i + 1 < SECRET_LEN {
                let v = (bits & 31) as usize;
                bits >>= 5;
                v
            } else {
                // 125 bits used: the last three of the pair and two of the third word.
                (bits & 7) as usize | (((entropy[2] & 3) as usize) << 3)
            };
            char::from(SECRET_ALPHABET[value])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vector `crypto.rs` and the meet Worker's suite pin (computed with node:crypto).
    const ROOM: &str = "a2h859hyqkfaa11nhzxfh3gd7f";
    const SECRET: &str = "k7qz2m9x4c8v1b6n3r5t0w2y8p";
    const INVITE_KEY: &str = "ccb342c6649ef79ce977c76bc83b944e778253cb1899167be279246c0f3774c1";

    #[test]
    fn a_secret_derives_the_invite_key_a_room_is_registered_with() {
        assert_eq!(invite_key(ROOM, SECRET).as_deref(), Some(INVITE_KEY));
        assert_ne!(
            invite_key(&"0".repeat(26), SECRET).as_deref(),
            Some(INVITE_KEY),
            "bound to its room"
        );
        assert_eq!(invite_key(ROOM, "short"), None);
        assert_eq!(invite_key(ROOM, "k7qz2m9x4c8v1b6n3r5t0w2y8u"), None, "u is not in it");
        assert_eq!(invite_key(ROOM, &SECRET.to_uppercase()), None, "lower case only");
    }

    #[test]
    fn three_random_words_make_a_secret_of_the_alphabet() {
        let secret = secret_from([u64::MAX, u64::MAX, 3]);
        assert!(is_secret(&secret), "{secret}");
        assert_eq!(secret, "z".repeat(26));
        assert_eq!(secret_from([0, 0, 0]), "0".repeat(26));
        assert_ne!(secret_from([1, 2, 3]), secret_from([1, 2, 2]), "the third word counts");
    }
}
