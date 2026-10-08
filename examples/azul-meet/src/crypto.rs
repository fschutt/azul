//! AzMeet's cryptography, as `CRYPTO.md` designs it: the device identity (Ed25519 for signatures,
//! X25519 for agreement, both from one seed in the OS keyring), safety codes, the invite secret of a
//! link (its proof key and its name key), room keys sealed to one member's X25519 key, messages
//! sealed with a room key, the canonical strings every signature covers, and the secrets sealed
//! into this device's own files. Pure: no azul types, no I/O but the OS random source; the tests
//! pin the vectors the meet Worker's suite pins too (`cf-workers/meet/test/chatrooms.test.mjs`), so
//! a signature made here verifies there.
//!
//! Secrets wipe themselves when dropped (`zeroize`) and never print: `Identity`, `Invite` and
//! `RoomKey` show only their public parts in `Debug`.

use std::fmt;

use azul_pim::data_uri::{decode_base64, encode_base64};
use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as DhPublic, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

/// The id alphabet (lower-case Crockford base32): room ids and invite secrets.
pub const ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
/// The characters of a room id or an invite secret (130 random bits).
pub const ID_LEN: usize = 26;
/// The bytes of a key, a seed, a public key.
pub const KEY_LEN: usize = 32;
/// The bytes of an XChaCha20-Poly1305 nonce, and of its tag.
pub const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
/// The JSON the seed is kept in (the keyring's secret, an identity file).
pub const IDENTITY_FORMAT: &str = "azmeet-identity";
/// A message's plaintext is padded to a multiple of this many bytes: lengths show in steps.
pub const PAD_STEP: usize = 128;
/// A name's plaintext is padded to a multiple of this many bytes.
const NAME_PAD_STEP: usize = 32;

/// Why something did not seal, open or verify. Never holds a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// The OS random source failed.
    Random,
    /// Not the shape it must have (a key that is no key, base64 that is not, ...).
    Malformed(&'static str),
    /// It did not open: another key, another recipient, another room, or changed bytes.
    Open,
    /// It opened, to a key that is not the key its record names.
    WrongKey,
    /// A format or version this build does not read.
    Unsupported,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::Random => f.write_str("the system's random source failed"),
            CryptoError::Malformed(what) => write!(f, "not {what}"),
            CryptoError::Open => f.write_str("it does not open with this key"),
            CryptoError::WrongKey => f.write_str("it opened to another key than its record names"),
            CryptoError::Unsupported => f.write_str("a format this AzMeet does not read"),
        }
    }
}

/// Fills `buf` from the OS random source.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), CryptoError> {
    getrandom::getrandom(buf).map_err(|_| CryptoError::Random)
}

/// 26 random characters of the id alphabet (130 bits): a room id, or an invite secret.
pub fn random_id() -> Result<String, CryptoError> {
    let mut bytes = [0u8; ID_LEN];
    random_bytes(&mut bytes)?;
    let id = bytes
        .iter()
        .map(|b| char::from(ID_ALPHABET[usize::from(b & 31)]))
        .collect();
    bytes.zeroize();
    Ok(id)
}

/// Whether `text` is 26 characters of the id alphabet: a room id, or an invite secret.
#[must_use]
pub fn is_id(text: &str) -> bool {
    text.len() == ID_LEN && text.bytes().all(|b| ID_ALPHABET.contains(&b))
}

/// A new message id: 16 random bytes in hex.
pub fn new_message_id() -> Result<String, CryptoError> {
    let mut bytes = [0u8; 16];
    random_bytes(&mut bytes)?;
    Ok(hex(&bytes))
}

/// `bytes` in lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

/// The bytes of hex `text` (either case); `None` for an odd length or a character that is no hex
/// digit.
#[must_use]
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    bytes
        .chunks(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/// 32 bytes of hex `text`: a public key, a device id.
#[must_use]
pub fn unhex32(text: &str) -> Option<[u8; KEY_LEN]> {
    let bytes = unhex(text)?;
    <[u8; KEY_LEN]>::try_from(bytes.as_slice()).ok()
}

/// Whether `text` is a device id (or an X25519 key): 64 lower-case hex digits.
#[must_use]
pub fn is_device(text: &str) -> bool {
    text.len() == 2 * KEY_LEN && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// SHA-256 of `data`.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// SHA-256 of `data` in hex.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&sha256(data))
}

/// HKDF-SHA256 of `ikm` with `salt` and `info`, 32 bytes.
fn hkdf(ikm: &[u8], salt: &[u8], info: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut okm = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut okm[..])
        .expect("HKDF-SHA256 gives 32 bytes");
    okm
}

/// Seals `plain` with `key` under a fresh random nonce, authenticating `aad`: the nonce and the
/// ciphertext with its tag.
fn aead_seal(
    key: &[u8; KEY_LEN],
    plain: &[u8],
    aad: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>), CryptoError> {
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce)?;
    let sealed = XChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| CryptoError::Malformed("something the cipher seals"))?;
    Ok((nonce, sealed))
}

/// Opens what [`aead_seal`] sealed; `Err(Open)` for another key, nonce, ciphertext or `aad`.
fn aead_open(
    key: &[u8; KEY_LEN],
    nonce: &[u8],
    sealed: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if nonce.len() != NONCE_LEN {
        return Err(CryptoError::Malformed("a nonce"));
    }
    XChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad })
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::Open)
}

/// `nonce || sealed` in base64: the form every sealed thing travels in.
fn seal_b64(key: &[u8; KEY_LEN], plain: &[u8], aad: &[u8]) -> Result<String, CryptoError> {
    let (nonce, sealed) = aead_seal(key, plain, aad)?;
    let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(encode_base64(&out))
}

/// Opens what [`seal_b64`] made.
fn open_b64(
    key: &[u8; KEY_LEN],
    text: &str,
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let bytes = decode_base64(text).ok_or(CryptoError::Malformed("base64"))?;
    if bytes.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::Malformed("something sealed"));
    }
    aead_open(key, &bytes[..NONCE_LEN], &bytes[NONCE_LEN..], aad)
}

/// `bytes` with spaces after them up to the next multiple of `step` (at least one step).
fn padded(bytes: &[u8], step: usize) -> Zeroizing<Vec<u8>> {
    let target = bytes.len().div_ceil(step).max(1) * step;
    let mut out = Zeroizing::new(Vec::with_capacity(target));
    out.extend_from_slice(bytes);
    out.resize(target, b' ');
    out
}

// ==== The canonical strings (CRYPTO.md section 2): what every signature and AAD covers ====

/// A signed request's text (CRYPTO.md section 11): the method, the path from `/rooms` on, the
/// time in milliseconds and the hash of the body as sent.
#[must_use]
pub fn request_input(method: &str, path: &str, ts: u64, body: &[u8]) -> String {
    format!(
        "azmeet/v1/request\n{method}\n{path}\n{ts}\n{}",
        sha256_hex(body)
    )
}

/// A member record's text (CRYPTO.md section 5); a missing field is an empty line.
#[must_use]
pub fn member_input(
    room: &str,
    device: &str,
    dh: &str,
    sealed_name: Option<&str>,
    name: Option<&str>,
    ts: u64,
) -> String {
    format!(
        "azmeet/v1/member\n{room}\n{device}\n{dh}\n{}\n{}\n{ts}",
        sealed_name.unwrap_or(""),
        name.unwrap_or("")
    )
}

/// What the link's invite key signs for a device that holds the link.
#[must_use]
pub fn proof_input(room: &str, device: &str, dh: &str) -> String {
    format!("azmeet/v1/proof\n{room}\n{device}\n{dh}")
}

/// What a member signs to let a knocking device in.
#[must_use]
pub fn admit_input(room: &str, device: &str, dh: &str) -> String {
    format!("azmeet/v1/admit\n{room}\n{device}\n{dh}")
}

/// A room key's record: its id, its epoch and the devices it was sealed to, sorted.
#[must_use]
pub fn key_input(room: &str, key_id: &str, epoch: u64, members: &[String]) -> String {
    let mut sorted: Vec<&str> = members.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    format!(
        "azmeet/v1/key\n{room}\n{key_id}\n{epoch}\n{}",
        sorted.join(",")
    )
}

/// A message's signed text: its id, its key and the hash of its base64 body.
#[must_use]
pub fn message_input(room: &str, id: &str, key_id: &str, body: &str) -> String {
    format!(
        "azmeet/v1/msg\n{room}\n{id}\n{key_id}\n{}",
        sha256_hex(body.as_bytes())
    )
}

/// An iroh announcement's signed text: the endpoint and its ticket.
#[must_use]
pub fn peer_input(room: &str, node_id: &str, ticket: &str) -> String {
    format!("azmeet/v1/peer\n{room}\n{node_id}\n{ticket}")
}

/// Whether `sig` (hex) is `device`'s Ed25519 signature over `text` (strict: no malleable
/// signatures, no small-order keys). False for a key or a signature that is none.
#[must_use]
pub fn verify(device: &str, sig: &str, text: &str) -> bool {
    let Some(public) = unhex32(device) else {
        return false;
    };
    let Some(sig) = unhex(sig).and_then(|bytes| <[u8; 64]>::try_from(bytes.as_slice()).ok()) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&public) else {
        return false;
    };
    key.verify_strict(text.as_bytes(), &Signature::from_bytes(&sig))
        .is_ok()
}

/// The safety code of the device with these public keys (CRYPTO.md section 3): 20 digits in four
/// groups, "05881 39114 50072 66310". `None` when a key is no key.
#[must_use]
pub fn safety_code(device: &str, dh: &str) -> Option<String> {
    let sign = unhex32(device)?;
    let agree = unhex32(dh)?;
    let mut input = Vec::with_capacity(17 + 2 * KEY_LEN);
    input.extend_from_slice(b"azmeet/v1/safety\n");
    input.extend_from_slice(&sign);
    input.extend_from_slice(&agree);
    let d = sha256(&input);
    let groups: Vec<String> = (0..4)
        .map(|i| {
            let n = d[5 * i..5 * i + 5]
                .iter()
                .fold(0u64, |n, b| n << 8 | u64::from(*b));
            format!("{:05}", n % 100_000)
        })
        .collect();
    Some(groups.join(" "))
}

// ==== The device identity (CRYPTO.md section 3) ====

/// This device: the seed and everything derived from it.
pub struct Identity {
    seed: Zeroizing<[u8; KEY_LEN]>,
    sign: SigningKey,
    dh: StaticSecret,
    local: Zeroizing<[u8; KEY_LEN]>,
    device: String,
    dh_public: String,
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identity({})", self.device)
    }
}

/// The JSON a seed is kept in.
#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    format: String,
    version: u32,
    seed: String,
}

impl Identity {
    /// The identity of `seed`.
    #[must_use]
    pub fn from_seed(seed: [u8; KEY_LEN]) -> Identity {
        let seed = Zeroizing::new(seed);
        let salt = b"azmeet/v1/identity";
        let sign = SigningKey::from_bytes(&hkdf(&seed[..], salt, b"ed25519"));
        let dh = StaticSecret::from(*hkdf(&seed[..], salt, b"x25519"));
        let local = hkdf(&seed[..], salt, b"local");
        let device = hex(sign.verifying_key().as_bytes());
        let dh_public = hex(DhPublic::from(&dh).as_bytes());
        Identity {
            seed,
            sign,
            dh,
            local,
            device,
            dh_public,
        }
    }

    /// A new identity from the OS random source.
    pub fn generate() -> Result<Identity, CryptoError> {
        let mut seed = [0u8; KEY_LEN];
        random_bytes(&mut seed)?;
        let identity = Identity::from_seed(seed);
        seed.zeroize();
        Ok(identity)
    }

    /// The device id: the Ed25519 public key in hex.
    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// The X25519 public key in hex.
    #[must_use]
    pub fn dh(&self) -> &str {
        &self.dh_public
    }

    /// This device's safety code.
    #[must_use]
    pub fn safety_code(&self) -> String {
        safety_code(&self.device, &self.dh_public).unwrap_or_default()
    }

    /// This device's Ed25519 signature over `text`, in hex.
    #[must_use]
    pub fn sign(&self, text: &str) -> String {
        hex(&self.sign.sign(text.as_bytes()).to_bytes())
    }

    /// The headers of a signed request (CRYPTO.md section 11): device, time, signature.
    #[must_use]
    pub fn request_headers(
        &self,
        method: &str,
        path: &str,
        ts: u64,
        body: &[u8],
    ) -> [(&'static str, String); 3] {
        [
            ("x-azmeet-device", self.device.clone()),
            ("x-azmeet-ts", ts.to_string()),
            (
                "x-azmeet-sig",
                self.sign(&request_input(method, path, ts, body)),
            ),
        ]
    }

    /// The seed as the keyring (or an identity file) keeps it.
    #[must_use]
    pub fn to_secret_json(&self) -> Zeroizing<String> {
        let mut seed = encode_base64(&self.seed[..]);
        let text =
            format!("{{\"format\":\"{IDENTITY_FORMAT}\",\"version\":1,\"seed\":\"{seed}\"}}");
        seed.zeroize();
        Zeroizing::new(text)
    }

    /// The identity a keyring secret or an identity file holds.
    pub fn from_secret_json(text: &str) -> Result<Identity, CryptoError> {
        let mut stored: StoredIdentity = serde_json::from_str(text.trim())
            .map_err(|_| CryptoError::Malformed("an AzMeet identity"))?;
        if stored.format != IDENTITY_FORMAT || stored.version != 1 {
            stored.seed.zeroize();
            return Err(CryptoError::Unsupported);
        }
        let bytes = Zeroizing::new(decode_base64(&stored.seed).unwrap_or_default());
        stored.seed.zeroize();
        let mut seed = <[u8; KEY_LEN]>::try_from(bytes.as_slice())
            .map_err(|_| CryptoError::Malformed("a 32-byte seed"))?;
        let identity = Identity::from_seed(seed);
        seed.zeroize();
        Ok(identity)
    }

    /// Seals `secret` (a room's invite secret) for this device's own files.
    pub fn seal_local(&self, room: &str, secret: &str) -> Result<String, CryptoError> {
        seal_b64(&self.local, secret.as_bytes(), local_aad(room).as_bytes())
    }

    /// Opens what [`Identity::seal_local`] sealed for `room`; `None` for another device or room.
    #[must_use]
    pub fn open_local(&self, room: &str, sealed: &str) -> Option<Zeroizing<String>> {
        let plain = open_b64(&self.local, sealed, local_aad(room).as_bytes()).ok()?;
        String::from_utf8(plain.to_vec()).ok().map(Zeroizing::new)
    }
}

fn local_aad(room: &str) -> String {
    format!("azmeet/v1/local\n{room}")
}

// ==== The link (CRYPTO.md section 4) ====

/// A room's invite secret and what is derived from it: the invite key (whose public half the room
/// is registered with) and the key that seals display names.
pub struct Invite {
    room: String,
    secret: Zeroizing<String>,
    sign: SigningKey,
    names: Zeroizing<[u8; KEY_LEN]>,
}

impl fmt::Debug for Invite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invite({}, {})", self.room, self.invite_key())
    }
}

impl Clone for Invite {
    fn clone(&self) -> Self {
        Invite {
            room: self.room.clone(),
            secret: self.secret.clone(),
            sign: SigningKey::from_bytes(&self.sign.to_bytes()),
            names: self.names.clone(),
        }
    }
}

impl Invite {
    /// The invite of room `room` with secret `secret` (26 characters of the id alphabet); `None`
    /// for a secret that is none.
    #[must_use]
    pub fn new(room: &str, secret: &str) -> Option<Invite> {
        if !is_id(secret) {
            return None;
        }
        let salt = b"azmeet/v1/invite";
        let sign = SigningKey::from_bytes(&hkdf(
            secret.as_bytes(),
            salt,
            format!("sign\n{room}").as_bytes(),
        ));
        let names = hkdf(secret.as_bytes(), salt, format!("names\n{room}").as_bytes());
        Some(Invite {
            room: room.to_string(),
            secret: Zeroizing::new(secret.to_string()),
            sign,
            names,
        })
    }

    /// A new random invite for `room`.
    pub fn generate(room: &str) -> Result<Invite, CryptoError> {
        let secret = Zeroizing::new(random_id()?);
        Invite::new(room, &secret).ok_or(CryptoError::Malformed("an invite secret"))
    }

    /// The room this invite is for.
    #[must_use]
    pub fn room(&self) -> &str {
        &self.room
    }

    /// The secret, for the link's fragment.
    #[must_use]
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// The public invite key in hex, as the room is registered with it.
    #[must_use]
    pub fn invite_key(&self) -> String {
        hex(self.sign.verifying_key().as_bytes())
    }

    /// The proof that the device with these keys holds the link.
    #[must_use]
    pub fn proof(&self, device: &str, dh: &str) -> String {
        hex(&self
            .sign
            .sign(proof_input(&self.room, device, dh).as_bytes())
            .to_bytes())
    }

    /// `name` sealed for every holder of the link, bound to `device` (base64).
    pub fn seal_name(&self, device: &str, name: &str) -> Result<String, CryptoError> {
        let plain = padded(name.as_bytes(), NAME_PAD_STEP);
        seal_b64(&self.names, &plain, name_aad(&self.room, device).as_bytes())
    }

    /// The name `device` sealed with [`Invite::seal_name`]; `None` when it does not open.
    #[must_use]
    pub fn open_name(&self, device: &str, sealed: &str) -> Option<String> {
        let plain = open_b64(&self.names, sealed, name_aad(&self.room, device).as_bytes()).ok()?;
        let text = std::str::from_utf8(&plain).ok()?;
        Some(text.trim_end_matches(' ').to_string())
    }
}

fn name_aad(room: &str, device: &str) -> String {
    format!("azmeet/v1/name\n{room}\n{device}")
}

// ==== Room keys (CRYPTO.md section 7) ====

/// A room key and its id.
#[derive(Clone)]
pub struct RoomKey {
    bytes: Zeroizing<[u8; KEY_LEN]>,
    id: String,
}

impl fmt::Debug for RoomKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RoomKey({})", self.id)
    }
}

impl PartialEq for RoomKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RoomKey {}

/// The id of key `bytes`: the first 16 bytes of `H("azmeet/v1/key-id\n" || key)` in hex.
#[must_use]
pub fn key_id_of(bytes: &[u8; KEY_LEN]) -> String {
    let mut input = Zeroizing::new(Vec::with_capacity(17 + KEY_LEN));
    input.extend_from_slice(b"azmeet/v1/key-id\n");
    input.extend_from_slice(bytes);
    hex(&sha256(&input)[..16])
}

impl RoomKey {
    /// A new random room key.
    pub fn generate() -> Result<RoomKey, CryptoError> {
        let mut bytes = [0u8; KEY_LEN];
        random_bytes(&mut bytes)?;
        let key = RoomKey::from_bytes(bytes);
        bytes.zeroize();
        Ok(key)
    }

    /// The room key of these bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> RoomKey {
        let id = key_id_of(&bytes);
        RoomKey {
            bytes: Zeroizing::new(bytes),
            id,
        }
    }

    /// The key's id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

fn seal_info(room: &str, key_id: &str, recipient: &str) -> String {
    format!("azmeet/v1/seal\n{room}\n{key_id}\n{recipient}")
}

fn seal_aad(room: &str, key_id: &str, epoch: u64, sender: &str, recipient: &str) -> String {
    format!("azmeet/v1/seal\n{room}\n{key_id}\n{epoch}\n{sender}\n{recipient}")
}

/// `key` and the room's invite secret sealed to `recipient` (its device id and X25519 key, both
/// hex): `b64(e_pk || nonce || sealed)`.
pub fn seal_key(
    room: &str,
    key: &RoomKey,
    epoch: u64,
    sender: &str,
    recipient: &str,
    recipient_dh: &str,
    invite_secret: &str,
) -> Result<String, CryptoError> {
    let their_bytes = unhex32(recipient_dh).ok_or(CryptoError::Malformed("an X25519 key"))?;
    let their = DhPublic::from(their_bytes);
    let mut ephemeral = Zeroizing::new([0u8; KEY_LEN]);
    random_bytes(&mut ephemeral[..])?;
    let e_sk = StaticSecret::from(*ephemeral);
    let e_pk = DhPublic::from(&e_sk);
    let shared = e_sk.diffie_hellman(&their);
    if !shared.was_contributory() {
        return Err(CryptoError::Malformed("an X25519 key of a large order"));
    }
    let mut salt = Vec::with_capacity(2 * KEY_LEN);
    salt.extend_from_slice(e_pk.as_bytes());
    salt.extend_from_slice(&their_bytes);
    let wrap = hkdf(
        shared.as_bytes(),
        &salt,
        seal_info(room, key.id(), recipient).as_bytes(),
    );
    let mut plain = Zeroizing::new(Vec::with_capacity(KEY_LEN + invite_secret.len()));
    plain.extend_from_slice(&key.bytes[..]);
    plain.extend_from_slice(invite_secret.as_bytes());
    let aad = seal_aad(room, key.id(), epoch, sender, recipient);
    let (nonce, sealed) = aead_seal(&wrap, &plain, aad.as_bytes())?;
    let mut out = Vec::with_capacity(KEY_LEN + NONCE_LEN + sealed.len());
    out.extend_from_slice(e_pk.as_bytes());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(encode_base64(&out))
}

/// This device's copy of room key `key_id` (of `epoch`, sent by `sender`), and the room's invite
/// secret it carries. `Err(WrongKey)` when it opens to another key than `key_id`.
pub fn open_key(
    me: &Identity,
    room: &str,
    key_id: &str,
    epoch: u64,
    sender: &str,
    sealed: &str,
) -> Result<(RoomKey, Zeroizing<String>), CryptoError> {
    let bytes = decode_base64(sealed).ok_or(CryptoError::Malformed("base64"))?;
    if bytes.len() < KEY_LEN + NONCE_LEN + TAG_LEN + KEY_LEN {
        return Err(CryptoError::Malformed("a sealed key"));
    }
    let mut e_bytes = [0u8; KEY_LEN];
    e_bytes.copy_from_slice(&bytes[..KEY_LEN]);
    let e_pk = DhPublic::from(e_bytes);
    let shared = me.dh.diffie_hellman(&e_pk);
    if !shared.was_contributory() {
        return Err(CryptoError::Open);
    }
    let mut salt = Vec::with_capacity(2 * KEY_LEN);
    salt.extend_from_slice(&e_bytes);
    salt.extend_from_slice(DhPublic::from(&me.dh).as_bytes());
    let wrap = hkdf(
        shared.as_bytes(),
        &salt,
        seal_info(room, key_id, &me.device).as_bytes(),
    );
    let aad = seal_aad(room, key_id, epoch, sender, &me.device);
    let plain = aead_open(
        &wrap,
        &bytes[KEY_LEN..KEY_LEN + NONCE_LEN],
        &bytes[KEY_LEN + NONCE_LEN..],
        aad.as_bytes(),
    )?;
    let mut key_bytes = [0u8; KEY_LEN];
    key_bytes.copy_from_slice(&plain[..KEY_LEN]);
    let key = RoomKey::from_bytes(key_bytes);
    key_bytes.zeroize();
    if key.id() != key_id {
        return Err(CryptoError::WrongKey);
    }
    let secret = std::str::from_utf8(&plain[KEY_LEN..])
        .ok()
        .filter(|s| is_id(s))
        .ok_or(CryptoError::Malformed("an invite secret"))?;
    Ok((key, Zeroizing::new(secret.to_string())))
}

// ==== Messages (CRYPTO.md section 8) ====

/// A message's plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plain {
    pub text: String,
    /// The sender's name when it wrote.
    pub name: String,
    /// When it wrote, milliseconds since 1970 (the sender's clock).
    pub ts: u64,
}

fn message_aad(room: &str, id: &str, key_id: &str, sender: &str) -> String {
    format!("azmeet/v1/msg\n{room}\n{id}\n{key_id}\n{sender}")
}

/// `plain` sealed with `key` as message `id` of `sender`, padded to a multiple of [`PAD_STEP`]
/// (base64).
pub fn seal_message(
    key: &RoomKey,
    room: &str,
    id: &str,
    sender: &str,
    plain: &Plain,
) -> Result<String, CryptoError> {
    let json = serde_json::to_vec(plain).map_err(|_| CryptoError::Malformed("a message"))?;
    let text = padded(&json, PAD_STEP);
    seal_b64(
        &key.bytes,
        &text,
        message_aad(room, id, key.id(), sender).as_bytes(),
    )
}

/// The plaintext of message `id` of `sender` sealed with `key`.
pub fn open_message(
    key: &RoomKey,
    room: &str,
    id: &str,
    sender: &str,
    body: &str,
) -> Result<Plain, CryptoError> {
    let plain = open_b64(
        &key.bytes,
        body,
        message_aad(room, id, key.id(), sender).as_bytes(),
    )?;
    serde_json::from_slice(&plain).map_err(|_| CryptoError::Malformed("a message"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The vectors the meet Worker's suite pins too (cf-workers/meet/test/chatrooms.test.mjs),
    // computed once with node:crypto: the seeds 00..1f (A) and 20..3f (B).
    const ROOM: &str = "a2h859hyqkfaa11nhzxfh3gd7f";
    const SECRET: &str = "k7qz2m9x4c8v1b6n3r5t0w2y8p";
    const TS: u64 = 1_760_000_000_000;
    const DEVICE_A: &str = "400061d5faf0118040fd32ee5d4e0fb91e9244b8ad7899762d2d7fb1af81b259";
    const DH_A: &str = "50df25dbb42b6914145052556245fa2f14d0f8d812420131094064569a88556e";
    const DEVICE_B: &str = "964b7270628bc5df67b19e439757c4a9e564aa98a6ab0e2fc3a51903c77602a9";
    const DH_B: &str = "0135ff365f7a79220628e4db56c463802d057ace13b39f9c6b00ef31fc36f448";
    const INVITE_KEY: &str = "ccb342c6649ef79ce977c76bc83b944e778253cb1899167be279246c0f3774c1";
    const KEY_ID: &str = "588cbfe46011380b5d9af6e54072d5df";

    fn seed(first: u8) -> [u8; 32] {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = first + i as u8;
        }
        seed
    }

    fn ada() -> Identity {
        Identity::from_seed(seed(0))
    }

    fn ben() -> Identity {
        Identity::from_seed(seed(32))
    }

    #[test]
    fn a_seed_derives_the_same_device_keys_and_safety_codes_as_the_workers_vectors() {
        let a = ada();
        assert_eq!(a.device(), DEVICE_A);
        assert_eq!(a.dh(), DH_A);
        assert_eq!(a.safety_code(), "59174 51299 37993 31242");
        let b = ben();
        assert_eq!(b.device(), DEVICE_B);
        assert_eq!(b.dh(), DH_B);
        assert_eq!(b.safety_code(), "72491 76408 78499 97266");
        assert_eq!(
            hex(&a.local[..]),
            "85d89c413a426ca13261fe9bff5a2cb2f80cdf8889058b55e270d6b0784ace2e"
        );
        assert_eq!(
            safety_code(DEVICE_B, DH_B).as_deref(),
            Some("72491 76408 78499 97266")
        );
        assert_eq!(safety_code("zz", DH_B), None);
    }

    #[test]
    fn every_signature_is_the_one_the_worker_verifies() {
        let a = ada();
        let body = br#"{"id":"00112233445566778899aabbccddeeff"}"#;
        assert_eq!(
            a.sign(&request_input("POST", &format!("/rooms/{ROOM}/messages"), TS, body)),
            "8e69669e69c9e8a8529ff2e67bd6a10959195a4f6e315bee0b901cdcafa8a124efb22c315c3d615ad8d98a9a88a664e9b2a7cba8ed3d3fcea0c8a064bb8f830c"
        );
        assert_eq!(
            a.sign(&request_input("DELETE", &format!("/rooms/{ROOM}/members/{DEVICE_A}"), TS, b"")),
            "8b5bf38c85af95b55474679168a18d6358eac69b0fabc81bf4f23eb424e11e0b67911cc99755a035c82721c58ea15fa3fb2b0172d6078dbe959936a571e50d05"
        );
        assert_eq!(
            a.sign(&member_input(ROOM, DEVICE_A, DH_A, Some("c2VhbGVk"), None, TS)),
            "ab41a5aa44e8249b6cd0f8bea0aee6bb85d3334e1d805240a9cf11d50ae2ed321def6afe352087ea0424b38b9fc913a9c9a904ce657fa2497af7422ec99c320a"
        );
        assert_eq!(
            ben().sign(&member_input(ROOM, DEVICE_B, DH_B, None, Some("Ben"), TS)),
            "d98b2acb18df5477bfde2f0a07104c95e1c104299e75acaaf7a7be36d44961c108f44cc1d5290b14e086bb9c02cf30e1608fe8ea23649f4cd3bd7b78e5712208"
        );
        assert_eq!(
            a.sign(&admit_input(ROOM, DEVICE_B, DH_B)),
            "2de8221943e4388e5e873f400e022fe616cb469ecf562db89b080673f104462771a27e22d578bcc0dc777ef324aff083e335ccbf45321ad4e66cb3f14100780f"
        );
        let members = vec![DEVICE_B.to_string(), DEVICE_A.to_string()];
        assert_eq!(
            a.sign(&key_input(ROOM, KEY_ID, 1, &members)),
            "e20783299026eb05b45c071d2f1a2d2608484e0cef0daa962e8caf0f4e492d56515ebc79c4cab212a446cc9cd49e1b95d7062cc215efaf16c2b568db3ffd9e03"
        );
        assert_eq!(
            a.sign(&message_input(ROOM, "00112233445566778899aabbccddeeff", KEY_ID, "Ym9keQ==")),
            "83647ddca715f9e0c7aad7ae186f0994b3a3217c6b5a05f99f48ee2892b8fe2a64d23e830a5d52a4777a6be245d9906bddbc39b37ae97a2b9413a948414d6609"
        );
        assert_eq!(
            a.sign(&peer_input(ROOM, &"aa".repeat(32), "endpointabc")),
            "989c7eb4802c4a07c4330b7a35cc33c1973e71b78ba8e2e5560d2fe8c71475c3c408a39a4211ab6e17d7dfba66fc2bbc8bff635c61c9d3ff3b2e7d12f6f0980c"
        );
    }

    #[test]
    fn the_invite_secret_derives_the_registered_key_its_proofs_and_the_name_key() {
        let invite = Invite::new(ROOM, SECRET).expect("a secret");
        assert_eq!(invite.invite_key(), INVITE_KEY);
        assert_eq!(
            invite.proof(DEVICE_A, DH_A),
            "24d457c793be0e6254f50421ef5f9bb2a15c29dad48773284ec15d20cb166b2d82ebf6b3e32620268355da8ff2bdf66bf5fee17e5a9fa3ecc9c156450199fd0f"
        );
        assert!(verify(
            INVITE_KEY,
            &invite.proof(DEVICE_B, DH_B),
            &proof_input(ROOM, DEVICE_B, DH_B)
        ));
        assert_eq!(
            hex(&invite.names[..]),
            "0f085eb16e6a4eb15361830d5d757647b875f59a894f999006a37a2970cb135d"
        );
        assert!(Invite::new(ROOM, "short").is_none());
        assert!(
            Invite::new(ROOM, "k7qz2m9x4c8v1b6n3r5t0w2y8u").is_none(),
            "u is not in the alphabet"
        );
        let other_room = Invite::new("0".repeat(26).as_str(), SECRET).unwrap();
        assert_ne!(other_room.invite_key(), INVITE_KEY, "bound to its room");
    }

    #[test]
    fn a_room_keys_id_and_the_x25519_agreement_match_the_vectors() {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = 64 + i as u8;
        }
        assert_eq!(key_id_of(&bytes), KEY_ID);
        assert_eq!(RoomKey::from_bytes(bytes).id(), KEY_ID);
        let shared = ada()
            .dh
            .diffie_hellman(&DhPublic::from(unhex32(DH_B).unwrap()));
        assert_eq!(
            hex(shared.as_bytes()),
            "737d83e3ff86f0a9017b2bae27b6b90fa49b37178762e1e62ac41f563ceb8823"
        );
    }

    #[test]
    fn a_signature_verifies_only_for_its_text_and_key() {
        let a = ada();
        let text = peer_input(ROOM, "node", "ticket");
        let sig = a.sign(&text);
        assert!(verify(DEVICE_A, &sig, &text));
        assert!(!verify(DEVICE_A, &sig, &format!("{text}x")));
        assert!(!verify(DEVICE_B, &sig, &text));
        assert!(!verify(DEVICE_A, &"ab".repeat(64), &text));
        assert!(!verify(DEVICE_A, "not hex", &text));
        assert!(!verify(&"00".repeat(31), &sig, &text));
        let headers = a.request_headers("PUT", "/rooms/x/members/y", TS, b"{}");
        assert_eq!(headers[0], ("x-azmeet-device", DEVICE_A.to_string()));
        assert_eq!(headers[1], ("x-azmeet-ts", TS.to_string()));
        assert!(verify(
            DEVICE_A,
            &headers[2].1,
            &request_input("PUT", "/rooms/x/members/y", TS, b"{}")
        ));
    }

    #[test]
    fn a_sealed_room_key_opens_for_its_recipient_only_and_carries_the_invite_secret() {
        let (a, b) = (ada(), ben());
        let key = RoomKey::generate().unwrap();
        let sealed = seal_key(ROOM, &key, 3, a.device(), b.device(), b.dh(), SECRET).unwrap();
        let (opened, secret) = open_key(&b, ROOM, key.id(), 3, a.device(), &sealed).unwrap();
        assert_eq!(opened, key);
        assert_eq!(&**secret, SECRET);
        assert_eq!(
            open_key(&a, ROOM, key.id(), 3, a.device(), &sealed).unwrap_err(),
            CryptoError::Open,
            "not Ada's copy"
        );
        assert_eq!(
            open_key(&b, ROOM, key.id(), 4, a.device(), &sealed).unwrap_err(),
            CryptoError::Open,
            "another epoch"
        );
        assert_eq!(
            open_key(&b, ROOM, key.id(), 3, b.device(), &sealed).unwrap_err(),
            CryptoError::Open,
            "another sender"
        );
        assert_eq!(
            open_key(
                &b,
                "0".repeat(26).as_str(),
                key.id(),
                3,
                a.device(),
                &sealed
            )
            .unwrap_err(),
            CryptoError::Open,
            "another room"
        );
        let other = RoomKey::generate().unwrap();
        assert_eq!(
            open_key(&b, ROOM, other.id(), 3, a.device(), &sealed).unwrap_err(),
            CryptoError::Open,
            "another key id"
        );
        let mut bytes = decode_base64(&sealed).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(
            open_key(&b, ROOM, key.id(), 3, a.device(), &encode_base64(&bytes)).unwrap_err(),
            CryptoError::Open,
            "a changed byte"
        );
        assert!(open_key(&b, ROOM, key.id(), 3, a.device(), "AAAA").is_err());
        assert!(seal_key(ROOM, &key, 3, a.device(), b.device(), "nope", SECRET).is_err());
    }

    #[test]
    fn a_copy_that_opens_to_another_key_than_its_record_names_is_refused() {
        let (a, b) = (ada(), ben());
        let key = RoomKey::generate().unwrap();
        // A copy sealed under one key id, but holding another key: what a forged record would be.
        let swapped = RoomKey {
            bytes: RoomKey::generate().unwrap().bytes,
            id: key.id().to_string(),
        };
        let sealed = seal_key(ROOM, &swapped, 1, a.device(), b.device(), b.dh(), SECRET).unwrap();
        assert_eq!(
            open_key(&b, ROOM, key.id(), 1, a.device(), &sealed).unwrap_err(),
            CryptoError::WrongKey
        );
    }

    #[test]
    fn a_message_opens_with_its_key_its_id_and_its_sender_and_shows_its_length_in_steps() {
        let key = RoomKey::generate().unwrap();
        let plain = Plain {
            text: String::from("Hello Ben, can you see me? Grüße"),
            name: String::from("Ada"),
            ts: TS,
        };
        let id = new_message_id().unwrap();
        let body = seal_message(&key, ROOM, &id, DEVICE_A, &plain).unwrap();
        assert_eq!(
            open_message(&key, ROOM, &id, DEVICE_A, &body).unwrap(),
            plain
        );
        assert!(!body.contains("Hello"), "{body}");
        let raw = decode_base64(&body).unwrap();
        assert_eq!(
            raw.len(),
            NONCE_LEN + PAD_STEP + TAG_LEN,
            "a short message is one step"
        );
        let other = RoomKey::generate().unwrap();
        assert_eq!(
            open_message(&other, ROOM, &id, DEVICE_A, &body).unwrap_err(),
            CryptoError::Open
        );
        assert_eq!(
            open_message(&key, ROOM, &new_message_id().unwrap(), DEVICE_A, &body).unwrap_err(),
            CryptoError::Open
        );
        assert_eq!(
            open_message(&key, ROOM, &id, DEVICE_B, &body).unwrap_err(),
            CryptoError::Open,
            "another sender"
        );
        let long = Plain {
            text: "x".repeat(300),
            ..plain
        };
        let long_body = seal_message(&key, ROOM, &id, DEVICE_A, &long).unwrap();
        assert_eq!(
            (decode_base64(&long_body).unwrap().len() - NONCE_LEN - TAG_LEN) % PAD_STEP,
            0
        );
        assert_eq!(
            open_message(&key, ROOM, &id, DEVICE_A, &long_body).unwrap(),
            long
        );
    }

    #[test]
    fn a_name_seals_for_the_holders_of_the_link_bound_to_its_device() {
        let invite = Invite::new(ROOM, SECRET).unwrap();
        let sealed = invite.seal_name(DEVICE_A, "Ada Lovelace").unwrap();
        assert!(!sealed.contains("Ada"));
        assert_eq!(
            invite.open_name(DEVICE_A, &sealed).as_deref(),
            Some("Ada Lovelace")
        );
        assert_eq!(
            invite.open_name(DEVICE_B, &sealed),
            None,
            "moved to another device"
        );
        let stranger = Invite::new(ROOM, &random_id().unwrap()).unwrap();
        assert_eq!(stranger.open_name(DEVICE_A, &sealed), None, "another link");
        assert_eq!(
            decode_base64(&invite.seal_name(DEVICE_A, "A").unwrap())
                .unwrap()
                .len(),
            decode_base64(&invite.seal_name(DEVICE_A, "Ada Lovelace").unwrap())
                .unwrap()
                .len(),
            "short names are as long as each other"
        );
    }

    #[test]
    fn the_seed_round_trips_through_its_json_and_a_wrong_one_is_refused() {
        let a = ada();
        let json = a.to_secret_json();
        assert!(json.starts_with("{\"format\":\"azmeet-identity\",\"version\":1,\"seed\":\""));
        let back = Identity::from_secret_json(&json).unwrap();
        assert_eq!(back.device(), DEVICE_A);
        assert_eq!(
            format!("{back:?}"),
            format!("Identity({DEVICE_A})"),
            "Debug shows no secret"
        );
        assert_eq!(
            Identity::from_secret_json(r#"{"format":"azmeet-identity","version":2,"seed":"AAAA"}"#)
                .unwrap_err(),
            CryptoError::Unsupported
        );
        assert!(Identity::from_secret_json(
            r#"{"format":"azmeet-identity","version":1,"seed":"AAAA"}"#
        )
        .is_err());
        assert!(Identity::from_secret_json("not json").is_err());
        let fresh = Identity::generate().unwrap();
        assert_ne!(fresh.device(), DEVICE_A);
        assert!(is_device(fresh.device()) && is_device(fresh.dh()));
    }

    #[test]
    fn a_secret_sealed_for_this_devices_files_opens_only_here_and_for_its_room() {
        let (a, b) = (ada(), ben());
        let sealed = a.seal_local(ROOM, SECRET).unwrap();
        assert!(!sealed.contains(SECRET));
        assert_eq!(
            a.open_local(ROOM, &sealed).as_deref().map(String::as_str),
            Some(SECRET)
        );
        assert!(b.open_local(ROOM, &sealed).is_none(), "another device");
        assert!(
            a.open_local("0".repeat(26).as_str(), &sealed).is_none(),
            "another room"
        );
    }

    #[test]
    fn ids_hex_and_the_alphabet() {
        let id = random_id().unwrap();
        assert!(is_id(&id), "{id}");
        assert_ne!(random_id().unwrap(), id);
        assert!(!is_id("a2h859hyqkfaa11nhzxfh3gd7u"));
        assert_eq!(hex(&[0, 15, 255]), "000fff");
        assert_eq!(unhex("000FfF"), Some(vec![0, 15, 255]));
        assert_eq!(unhex("abc"), None);
        assert_eq!(unhex("zz"), None);
        assert_eq!(new_message_id().unwrap().len(), 32);
        assert!(is_device(DEVICE_A));
        assert!(!is_device(&DEVICE_A.to_uppercase()));
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
