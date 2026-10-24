// AZD1: one incoming message sealed to an encrypted drive's DROP KEY, so the bucket - and
// whoever reads it - holds ciphertext only. Web Crypto only (Cloudflare Workers, Node 20+,
// browsers): X25519, HKDF-SHA256, AES-256-GCM, SHA-256.
//
//   bytes   0   4  magic "AZD1"
//           4   1  version 1
//           5  16  the drop key's id: SHA-256("Azlin AZD1 drop key id" || drop public key)[..16]
//          21  32  E: a fresh X25519 public key, made for this message only
//          53  12  the AES-GCM nonce (random; the key is new per message anyway)
//          65   *  AES-256-GCM(plaintext), then its 16-byte tag
//
//   key        = HKDF-SHA256(ikm = X25519(e, drop public key), salt = E || drop public key,
//                            info = "Azlin AZD1 drop v1", 32 bytes)
//   aad        = bytes 0..65 || u32 LE length || drive id || u32 LE length || object key
//   plaintext  = {"v":1,"received":<seconds since 1970>,"folder":"Inbox"|"Spam"} "\n" raw message
//   object key = ".azlin/drop/" + 32 hex digits of 128 random bits (no address, date or
//                subject; under `.azlin/`, so it never meets a folder of the user's)
//
// The drive's devices hold the drop key's secret (sealed with the drive key in the bucket's
// `.azlin/keys/_drop.key`); one of them opens each drop, files the message into the drive's
// encrypted mail folders and deletes the drop. azul-storage's `crypto::drops` is the Rust side;
// scripts/azlin_drop.py the Python reference; test/azd1-vector.json the vector all three check.

const encoder = new TextEncoder();

export const MAGIC = encoder.encode("AZD1");
export const VERSION = 1;
export const KEY_ID_LEN = 16;
export const PUBLIC_LEN = 32;
export const NONCE_LEN = 12;
export const HEADER_LEN = 4 + 1 + KEY_ID_LEN + PUBLIC_LEN + NONCE_LEN;
export const DROP_PREFIX = ".azlin/drop/";
export const FOLDERS = ["Inbox", "Spam"];

const KEY_ID_LABEL = encoder.encode("Azlin AZD1 drop key id");
const HKDF_INFO = encoder.encode("Azlin AZD1 drop v1");
// PKCS #8 of an X25519 private key without its 32 bytes (RFC 8410).
const PKCS8_X25519_PREFIX = hexToBytes("302e020100300506032b656e04220420");

export function hexToBytes(hex) {
  if (typeof hex !== "string" || hex.length % 2 !== 0 || /[^0-9a-fA-F]/.test(hex)) {
    throw new Error("not hex");
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  }
  return out;
}

export function bytesToHex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

function lengthPrefixed(bytes) {
  const out = new Uint8Array(4 + bytes.length);
  new DataView(out.buffer).setUint32(0, bytes.length, true);
  out.set(bytes, 4);
  return out;
}

function base64UrlToBytes(text) {
  const b64 = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(b64 + "===".slice((b64.length + 3) % 4));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

/** The id of a drop public key (16 bytes). */
export async function dropKeyId(dropPublicKey) {
  const digest = await crypto.subtle.digest("SHA-256", concat(KEY_ID_LABEL, dropPublicKey));
  return new Uint8Array(digest).slice(0, KEY_ID_LEN);
}

/** The associated data of a drop: its header, the drive's id and the drop's object key. */
export function associatedData(header, driveId, objectKey) {
  return concat(
    header,
    lengthPrefixed(encoder.encode(driveId)),
    lengthPrefixed(encoder.encode(objectKey)),
  );
}

/** The sealed plaintext: the one-line JSON header, a newline, the raw message. */
export function plaintextOf(received, folder, raw) {
  if (!Number.isSafeInteger(received) || received < 0) {
    throw new Error("received must be whole seconds since 1970");
  }
  if (!FOLDERS.includes(folder)) {
    throw new Error(`folder must be one of ${FOLDERS.join(", ")}`);
  }
  return concat(encoder.encode(JSON.stringify({ v: 1, received, folder }) + "\n"), raw);
}

/** A new drop's object key: ".azlin/drop/" and 32 hex digits of 128 random bits. */
export function newDropObjectKey() {
  return DROP_PREFIX + bytesToHex(crypto.getRandomValues(new Uint8Array(16)));
}

/** An X25519 private key from its 32 bytes (the test vector's fixed keys). */
export async function importX25519Secret(secret) {
  return crypto.subtle.importKey(
    "pkcs8",
    concat(PKCS8_X25519_PREFIX, secret),
    { name: "X25519" },
    true,
    ["deriveBits"],
  );
}

/** The 32-byte public key of an X25519 private key. */
export async function x25519PublicOf(privateKey) {
  const jwk = await crypto.subtle.exportKey("jwk", privateKey);
  return base64UrlToBytes(jwk.x);
}

async function aesKey(shared, ephemeralPublic, dropPublicKey, usage) {
  if (shared.every((b) => b === 0)) {
    throw new Error("an X25519 key of low order");
  }
  const ikm = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
  return crypto.subtle.deriveKey(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: concat(ephemeralPublic, dropPublicKey),
      info: HKDF_INFO,
    },
    ikm,
    { name: "AES-GCM", length: 256 },
    false,
    [usage],
  );
}

async function sharedSecret(privateKey, publicKey) {
  const peer = await crypto.subtle.importKey("raw", publicKey, { name: "X25519" }, false, []);
  return new Uint8Array(
    await crypto.subtle.deriveBits({ name: "X25519", public: peer }, privateKey, 256),
  );
}

/**
 * The AZD1 bytes of `raw` (the RFC 5322 message) for the drive `driveId`, to be stored at
 * `objectKey`. `ephemeralSecret` and `nonce` are for the test vector only: left out, they are
 * fresh random values (as they must be for real mail).
 */
export async function sealDrop({
  dropPublicKey,
  driveId,
  objectKey,
  received,
  folder,
  raw,
  ephemeralSecret,
  nonce,
}) {
  if (!(dropPublicKey instanceof Uint8Array) || dropPublicKey.length !== PUBLIC_LEN) {
    throw new Error("the drop public key is 32 bytes");
  }
  let ephemeral;
  let ephemeralPublic;
  if (ephemeralSecret) {
    ephemeral = await importX25519Secret(ephemeralSecret);
    ephemeralPublic = await x25519PublicOf(ephemeral);
  } else {
    const pair = await crypto.subtle.generateKey({ name: "X25519" }, true, ["deriveBits"]);
    ephemeral = pair.privateKey;
    ephemeralPublic = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  }
  const iv = nonce ?? crypto.getRandomValues(new Uint8Array(NONCE_LEN));
  if (iv.length !== NONCE_LEN) {
    throw new Error("the nonce is 12 bytes");
  }
  const key = await aesKey(
    await sharedSecret(ephemeral, dropPublicKey),
    ephemeralPublic,
    dropPublicKey,
    "encrypt",
  );
  const header = concat(
    MAGIC,
    Uint8Array.of(VERSION),
    await dropKeyId(dropPublicKey),
    ephemeralPublic,
    iv,
  );
  const sealed = await crypto.subtle.encrypt(
    {
      name: "AES-GCM",
      iv,
      additionalData: associatedData(header, driveId, objectKey),
      tagLength: 128,
    },
    key,
    plaintextOf(received, folder, raw),
  );
  return concat(header, new Uint8Array(sealed));
}

/**
 * Opens AZD1 `bytes` with the drop key's secret (an X25519 CryptoKey): `{ received, folder,
 * raw }`. The devices do this in Rust; here for the tests and for debugging.
 */
export async function openDrop({ bytes, dropSecret, driveId, objectKey }) {
  if (bytes.length < HEADER_LEN + 16) {
    throw new Error("not an AZD1 drop: too short");
  }
  if (bytesToHex(bytes.slice(0, 4)) !== bytesToHex(MAGIC) || bytes[4] !== VERSION) {
    throw new Error("not an AZD1 drop of version 1");
  }
  const dropPublicKey = await x25519PublicOf(dropSecret);
  const keyId = bytes.slice(5, 5 + KEY_ID_LEN);
  if (bytesToHex(keyId) !== bytesToHex(await dropKeyId(dropPublicKey))) {
    throw new Error("sealed to another drop key");
  }
  const ephemeralPublic = bytes.slice(21, 21 + PUBLIC_LEN);
  const iv = bytes.slice(53, HEADER_LEN);
  const header = bytes.slice(0, HEADER_LEN);
  const key = await aesKey(
    await sharedSecret(dropSecret, ephemeralPublic),
    ephemeralPublic,
    dropPublicKey,
    "decrypt",
  );
  const plain = new Uint8Array(
    await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv,
        additionalData: associatedData(header, driveId, objectKey),
        tagLength: 128,
      },
      key,
      bytes.slice(HEADER_LEN),
    ),
  );
  const newline = plain.indexOf(10);
  if (newline < 0) {
    throw new Error("a drop without its header line");
  }
  const head = JSON.parse(new TextDecoder().decode(plain.slice(0, newline)));
  if (head.v !== 1 || !FOLDERS.includes(head.folder) || !Number.isSafeInteger(head.received)) {
    throw new Error("a drop header of another version");
  }
  return { received: head.received, folder: head.folder, raw: plain.slice(newline + 1) };
}
