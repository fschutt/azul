#!/usr/bin/env python3
"""Reads a drive's SEALED period vault on this computer (AZ-K13), for the E2E scripts.

AzDrive keeps each drive's period vault - the issue keys of paid checkouts and the period tokens -
as `<drives file's folder>/period-tokens/<drive id>.json`, but sealed: `AZB1`, a 24-byte nonce and
XChaCha20-Poly1305's ciphertext, under a random 256-bit key that lives in the keyring entry
`azul-storage/vault-key/<drive id>` (base64url, no padding) and nowhere else. The associated data is
`azul-storage/blob/v1|azcloud-kit/period-vault|<drive id>` (examples/azul-storage/src/crypto/blob.rs).
A vault an older version wrote is plain JSON and is read as it is.

Pure Python 3 (the standard library has no ChaCha): XChaCha20-Poly1305 as in
draft-irtf-cfrg-xchacha, checked against its test vector by `python3 scripts/azlin_vault.py`.
"""
import base64
import json
import struct
import sys

MAGIC = b"AZB1"
AAD_LABEL = b"azul-storage/blob/v1|"
_MASK = 0xFFFFFFFF


def _rotl(value, count):
    return ((value << count) & _MASK) | (value >> (32 - count))


def _quarter(state, a, b, c, d):
    state[a] = (state[a] + state[b]) & _MASK
    state[d] = _rotl(state[d] ^ state[a], 16)
    state[c] = (state[c] + state[d]) & _MASK
    state[b] = _rotl(state[b] ^ state[c], 12)
    state[a] = (state[a] + state[b]) & _MASK
    state[d] = _rotl(state[d] ^ state[a], 8)
    state[c] = (state[c] + state[d]) & _MASK
    state[b] = _rotl(state[b] ^ state[c], 7)


def _rounds(state):
    work = list(state)
    for _ in range(10):
        _quarter(work, 0, 4, 8, 12)
        _quarter(work, 1, 5, 9, 13)
        _quarter(work, 2, 6, 10, 14)
        _quarter(work, 3, 7, 11, 15)
        _quarter(work, 0, 5, 10, 15)
        _quarter(work, 1, 6, 11, 12)
        _quarter(work, 2, 7, 8, 13)
        _quarter(work, 3, 4, 9, 14)
    return work


_CONSTANTS = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574]


def _block(key, counter, nonce12):
    state = _CONSTANTS + list(struct.unpack("<8I", key)) + [counter] + list(struct.unpack("<3I", nonce12))
    work = _rounds(state)
    return struct.pack("<16I", *[(w + s) & _MASK for w, s in zip(work, state)])


def _hchacha20(key, nonce16):
    state = _CONSTANTS + list(struct.unpack("<8I", key)) + list(struct.unpack("<4I", nonce16))
    work = _rounds(state)
    return struct.pack("<8I", *(work[0:4] + work[12:16]))


def _stream_xor(key, nonce12, counter, data):
    out = bytearray()
    for offset in range(0, len(data), 64):
        block = _block(key, counter + offset // 64, nonce12)
        chunk = data[offset:offset + 64]
        out.extend(a ^ b for a, b in zip(chunk, block))
    return bytes(out)


def _poly1305(key, message):
    r = int.from_bytes(key[:16], "little") & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
    s = int.from_bytes(key[16:32], "little")
    p = (1 << 130) - 5
    acc = 0
    for offset in range(0, len(message), 16):
        chunk = message[offset:offset + 16]
        acc = ((acc + int.from_bytes(chunk + b"\x01", "little")) * r) % p
    return ((acc + s) & ((1 << 128) - 1)).to_bytes(16, "little")


def _pad16(data):
    return b"\x00" * (-len(data) % 16)


def xchacha20poly1305_open(key, nonce24, sealed, aad):
    """The plaintext of `sealed` (ciphertext + 16-byte tag); ValueError when it does not authenticate."""
    if len(key) != 32 or len(nonce24) != 24 or len(sealed) < 16:
        raise ValueError("a key, a nonce or a sealed text of the wrong length")
    subkey = _hchacha20(key, nonce24[:16])
    nonce12 = b"\x00\x00\x00\x00" + nonce24[16:]
    ciphertext, tag = sealed[:-16], sealed[-16:]
    one_time = _block(subkey, 0, nonce12)[:32]
    mac_data = (aad + _pad16(aad) + ciphertext + _pad16(ciphertext)
                + struct.pack("<QQ", len(aad), len(ciphertext)))
    expected = _poly1305(one_time, mac_data)
    if not _constant_time_equal(expected, tag):
        raise ValueError("the sealed text does not authenticate")
    return _stream_xor(subkey, nonce12, 1, ciphertext)


def _constant_time_equal(a, b):
    result = 0
    for x, y in zip(a, b):
        result |= x ^ y
    return result == 0 and len(a) == len(b)


def open_sealed(key_text, drive_id, data, role="vault"):
    """The JSON object a sealed vault file holds; `key_text` is the keyring entry's text."""
    key = base64.urlsafe_b64decode(key_text.strip() + "=" * (-len(key_text.strip()) % 4))
    if not data.startswith(MAGIC) or len(data) < len(MAGIC) + 24 + 16:
        raise ValueError("it is no sealed vault file")
    nonce, sealed = data[4:28], data[28:]
    aad = AAD_LABEL + ("azcloud-kit/period-%s|%s" % (role, drive_id)).encode()
    return json.loads(xchacha20poly1305_open(key, nonce, sealed, aad))


def read_vault(path, drive_id, keyring_entries):
    """The vault at `path` as a dict. `keyring_entries` is a function from a keyring file's path to
    its {entry: text}; here it is called as `keyring_entries()` for the one the script reads.
    A plain file (an older version) is read as it is."""
    with open(path, "rb") as handle:
        data = handle.read()
    if not data.startswith(MAGIC):
        return json.loads(data)
    key_text = keyring_entries().get("azul-storage/vault-key/" + drive_id)
    if not key_text:
        raise ValueError("the vault is sealed and the keyring has no key for %s" % drive_id)
    return open_sealed(key_text, drive_id, data)


def _self_test():
    # draft-irtf-cfrg-xchacha-03, appendix A.3.1.
    key = bytes.fromhex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
    nonce = bytes.fromhex("404142434445464748494a4b4c4d4e4f5051525354555657")
    aad = bytes.fromhex("50515253c0c1c2c3c4c5c6c7")
    sealed = bytes.fromhex(
        "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb"
        "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452"
        "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9"
        "21f9664c97637da9768812f615c68b13b52e"
        "c0875924c1c7987947deafd8780acf49")
    plain = xchacha20poly1305_open(key, nonce, sealed, aad)
    expected = bytes.fromhex(
        "4c616469657320616e642047656e746c656d656e206f662074686520636c6173"
        "73206f66202739393a204966204920636f756c64206f6666657220796f75206f"
        "6e6c79206f6e652074697020666f7220746865206675747572652c2073756e73"
        "637265656e20776f756c642062652069742e")
    assert plain == expected, plain
    tampered = bytearray(sealed)
    tampered[3] ^= 1
    try:
        xchacha20poly1305_open(key, nonce, bytes(tampered), aad)
    except ValueError:
        pass
    else:
        raise AssertionError("a changed text opened")
    print("azlin_vault: the XChaCha20-Poly1305 test vector opens, a changed text does not")


if __name__ == "__main__":
    _self_test()
    sys.exit(0)
