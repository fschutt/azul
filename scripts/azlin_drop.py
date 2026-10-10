#!/usr/bin/env python3
"""AZD1, one incoming message sealed to an encrypted Azlin drive's DROP KEY, in Python's
standard library only: the reference the Cloudflare Email Worker (examples/azlin-mail-worker,
Web Crypto) and azul-storage's `crypto::drop` (Rust) are checked against.

    bytes   0   4  magic "AZD1"
            4   1  version 1
            5  16  the drop key's id: SHA-256("Azlin AZD1 drop key id" || drop public key)[:16]
           21  32  E: a fresh X25519 public key, made for this message only
           53  12  the AES-GCM nonce
           65   *  AES-256-GCM(plaintext) || the 16-byte tag

    key        = HKDF-SHA256(ikm = X25519(e, drop_pk), salt = E || drop_pk,
                             info = b"Azlin AZD1 drop v1"), 32 bytes
    aad        = bytes[0:65] || u32le(len(drive id)) || drive id || u32le(len(key)) || key
    plaintext  = {"v":1,"received":<seconds>,"folder":"Inbox"|"Spam"} b"\\n" raw RFC 5322 bytes
    object key = ".azlin/drop/" + 32 hex digits of 128 random bits

X25519 (RFC 7748) and HKDF (RFC 5869) come from scripts/azlin_claim.py; AES-256 (FIPS 197) and
GCM (SP 800-38D) are written out below and checked against their published vectors:

    python3 scripts/azlin_drop.py            # FIPS / GCM vectors, a seal / open round trip
    python3 scripts/azlin_drop.py --vector   # examples/azlin-mail-worker/test/azd1-vector.json
    python3 scripts/azlin_drop.py --open FILE --secret HEX --drive ID --key .azlin/drop/...

For tests only: nothing here is constant-time, and it is no general-purpose crypto library.
"""
import hashlib
import json
import os
import secrets
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from azlin_claim import hkdf_sha256, public_key, x25519  # noqa: E402

MAGIC = b'AZD1'
VERSION = 1
KEY_ID_LEN = 16
NONCE_LEN = 12
TAG_LEN = 16
HEADER_LEN = 4 + 1 + KEY_ID_LEN + 32 + NONCE_LEN
KEY_ID_LABEL = b'Azlin AZD1 drop key id'
HKDF_INFO = b'Azlin AZD1 drop v1'
FOLDERS = ('Inbox', 'Spam')
VECTOR_FILE = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    '..', 'examples', 'azlin-mail-worker', 'test', 'azd1-vector.json')

# ---- AES-256 (FIPS 197) ----


def _xtime(a):
    a <<= 1
    return (a ^ 0x11B) & 0xFF if a & 0x100 else a


def _gmul(a, b):
    out = 0
    while b:
        if b & 1:
            out ^= a
        a = _xtime(a)
        b >>= 1
    return out


def _make_sbox():
    sbox = [0] * 256
    for x in range(256):
        # The multiplicative inverse in GF(2^8) (0 for 0), then the affine map.
        inv = 0
        if x:
            inv = next(y for y in range(1, 256) if _gmul(x, y) == 1)
        s = inv
        for shift in range(1, 5):
            s ^= ((inv << shift) | (inv >> (8 - shift))) & 0xFF
        sbox[x] = s ^ 0x63
    return sbox


_SBOX = _make_sbox()


def _expand_key(key):
    assert len(key) == 32
    words = [list(key[4 * i:4 * i + 4]) for i in range(8)]
    rcon = 1
    for i in range(8, 60):
        temp = list(words[i - 1])
        if i % 8 == 0:
            temp = temp[1:] + temp[:1]
            temp = [_SBOX[b] for b in temp]
            temp[0] ^= rcon
            rcon = _xtime(rcon)
        elif i % 8 == 4:
            temp = [_SBOX[b] for b in temp]
        words.append([a ^ b for a, b in zip(words[i - 8], temp)])
    return [sum(words[4 * r:4 * r + 4], []) for r in range(15)]


def aes256_encrypt_block(round_keys, block):
    state = [b ^ k for b, k in zip(block, round_keys[0])]
    for r in range(1, 15):
        state = [_SBOX[b] for b in state]
        # ShiftRows: the state is column-major (byte 4c + row).
        state = [state[(4 * ((c + row) % 4)) + row] for c in range(4) for row in range(4)]
        if r != 14:
            mixed = []
            for c in range(4):
                a = state[4 * c:4 * c + 4]
                mixed += [
                    _gmul(a[0], 2) ^ _gmul(a[1], 3) ^ a[2] ^ a[3],
                    a[0] ^ _gmul(a[1], 2) ^ _gmul(a[2], 3) ^ a[3],
                    a[0] ^ a[1] ^ _gmul(a[2], 2) ^ _gmul(a[3], 3),
                    _gmul(a[0], 3) ^ a[1] ^ a[2] ^ _gmul(a[3], 2),
                ]
            state = mixed
        state = [b ^ k for b, k in zip(state, round_keys[r])]
    return bytes(state)

# ---- GCM (SP 800-38D), 96-bit nonces ----


def _gf128_mul(x, y):
    r = 0xE1 << 120
    z = 0
    v = y
    for i in range(127, -1, -1):
        if (x >> i) & 1:
            z ^= v
        v = (v >> 1) ^ r if v & 1 else v >> 1
    return z


def _ghash(h, aad, ciphertext):
    def blocks(data):
        for i in range(0, len(data), 16):
            yield data[i:i + 16].ljust(16, b'\0')
    y = 0
    for block in list(blocks(aad)) + list(blocks(ciphertext)):
        y = _gf128_mul(y ^ int.from_bytes(block, 'big'), h)
    lengths = struct.pack('>QQ', 8 * len(aad), 8 * len(ciphertext))
    return _gf128_mul(y ^ int.from_bytes(lengths, 'big'), h)


def _gctr(round_keys, counter_block, data):
    out = bytearray()
    counter = int.from_bytes(counter_block[12:], 'big')
    for i in range(0, len(data), 16):
        block = counter_block[:12] + struct.pack('>I', counter)
        stream = aes256_encrypt_block(round_keys, block)
        out += bytes(a ^ b for a, b in zip(data[i:i + 16], stream))
        counter = (counter + 1) & 0xFFFFFFFF
    return bytes(out)


def aes256_gcm_encrypt(key, nonce, plaintext, aad):
    assert len(nonce) == NONCE_LEN
    round_keys = _expand_key(key)
    h = int.from_bytes(aes256_encrypt_block(round_keys, bytes(16)), 'big')
    j0 = nonce + b'\0\0\0\1'
    ciphertext = _gctr(round_keys, nonce + b'\0\0\0\2', plaintext)
    s = _ghash(h, aad, ciphertext)
    tag = bytes(a ^ b for a, b in zip(aes256_encrypt_block(round_keys, j0), s.to_bytes(16, 'big')))
    return ciphertext + tag


def aes256_gcm_decrypt(key, nonce, sealed, aad):
    if len(sealed) < TAG_LEN:
        raise ValueError('cut short')
    ciphertext, tag = sealed[:-TAG_LEN], sealed[-TAG_LEN:]
    round_keys = _expand_key(key)
    h = int.from_bytes(aes256_encrypt_block(round_keys, bytes(16)), 'big')
    s = _ghash(h, aad, ciphertext)
    want = bytes(a ^ b for a, b in zip(
        aes256_encrypt_block(round_keys, nonce + b'\0\0\0\1'), s.to_bytes(16, 'big')))
    if not secrets.compare_digest(want, tag):
        raise ValueError('does not authenticate')
    return _gctr(round_keys, nonce + b'\0\0\0\2', ciphertext)

# ---- AZD1 ----


def drop_key_id(drop_pk):
    return hashlib.sha256(KEY_ID_LABEL + drop_pk).digest()[:KEY_ID_LEN]


def _length_prefixed(data):
    return struct.pack('<I', len(data)) + data


def associated_data(header, drive_id, object_key):
    return header + _length_prefixed(drive_id.encode()) + _length_prefixed(object_key.encode())


def plaintext_of(received, folder, raw):
    if folder not in FOLDERS:
        raise ValueError('folder')
    line = json.dumps({'v': 1, 'received': int(received), 'folder': folder},
                      separators=(',', ':'))
    return line.encode() + b'\n' + raw


def _aes_key(shared, eph_pk, drop_pk):
    if shared == bytes(32):
        raise ValueError('an X25519 key of low order')
    return hkdf_sha256(shared, eph_pk + drop_pk, HKDF_INFO, 32)


def seal(drop_pk, drive_id, object_key, received, folder, raw, eph_secret=None, nonce=None):
    eph_secret = eph_secret or secrets.token_bytes(32)
    nonce = nonce or secrets.token_bytes(NONCE_LEN)
    eph_pk = public_key(eph_secret)
    key = _aes_key(x25519(eph_secret, drop_pk), eph_pk, drop_pk)
    header = MAGIC + bytes([VERSION]) + drop_key_id(drop_pk) + eph_pk + nonce
    return header + aes256_gcm_encrypt(
        key, nonce, plaintext_of(received, folder, raw),
        associated_data(header, drive_id, object_key))


def open_drop(data, drop_secret, drive_id, object_key):
    if len(data) < HEADER_LEN + TAG_LEN or data[:4] != MAGIC or data[4] != VERSION:
        raise ValueError('not an AZD1 drop of version 1')
    drop_pk = public_key(drop_secret)
    if data[5:21] != drop_key_id(drop_pk):
        raise ValueError('sealed to another drop key')
    eph_pk, nonce, header = data[21:53], data[53:65], data[:HEADER_LEN]
    key = _aes_key(x25519(drop_secret, eph_pk), eph_pk, drop_pk)
    plain = aes256_gcm_decrypt(key, nonce, data[HEADER_LEN:],
                               associated_data(header, drive_id, object_key))
    line, _, raw = plain.partition(b'\n')
    head = json.loads(line)
    if head.get('v') != 1 or head.get('folder') not in FOLDERS:
        raise ValueError('a drop header of another version')
    return head['received'], head['folder'], raw


def new_object_key():
    return '.azlin/drop/' + secrets.token_hex(16)

# ---- checks ----


def self_test():
    # FIPS 197, appendix C.3 (AES-256).
    rk = _expand_key(bytes(range(32)))
    got = aes256_encrypt_block(rk, bytes.fromhex('00112233445566778899aabbccddeeff'))
    assert got.hex() == '8ea2b7ca516745bfeafc49904b496089', got.hex()
    # The GCM specification's test cases 13 and 14 (AES-256, zero key and nonce).
    zero_key, zero_nonce = bytes(32), bytes(12)
    assert aes256_gcm_encrypt(zero_key, zero_nonce, b'', b'').hex() == \
        '530f8afbc74536b9a963b4f1c4cb738b'
    assert aes256_gcm_encrypt(zero_key, zero_nonce, bytes(16), b'').hex() == \
        'cea7403d4d606b6e074ec5d3baf39d18' 'd0d1c8a799996bf0265b98b5d48ab919'
    # A round trip, and every refusal.
    drop_secret = secrets.token_bytes(32)
    drop_pk = public_key(drop_secret)
    key = new_object_key()
    raw = b'Subject: hi\r\n\r\nhello\r\n'
    sealed = seal(drop_pk, 'd_1', key, 1791619200, 'Spam', raw)
    assert open_drop(sealed, drop_secret, 'd_1', key) == (1791619200, 'Spam', raw)
    for drive, other_key in (('d_2', key), ('d_1', new_object_key())):
        try:
            open_drop(sealed, drop_secret, drive, other_key)
            raise AssertionError('opened for another drive or object')
        except ValueError:
            pass
    flipped = bytearray(sealed)
    flipped[-1] ^= 1
    try:
        open_drop(bytes(flipped), drop_secret, 'd_1', key)
        raise AssertionError('a changed drop opened')
    except ValueError:
        pass
    print('azlin_drop: FIPS 197, GCM and round-trip checks passed')


def check_vector(path=VECTOR_FILE):
    with open(path) as f:
        v = json.load(f)
    drop_secret = bytes.fromhex(v['drop_secret'])
    assert public_key(drop_secret).hex() == v['drop_public']
    assert drop_key_id(bytes.fromhex(v['drop_public'])).hex() == v['drop_key_id']
    assert public_key(bytes.fromhex(v['ephemeral_secret'])).hex() == v['ephemeral_public']
    raw = v['raw'].encode()
    assert plaintext_of(v['received'], v['folder'], raw).hex() == v['plaintext']
    sealed = seal(bytes.fromhex(v['drop_public']), v['drive_id'], v['object_key'],
                  v['received'], v['folder'], raw,
                  eph_secret=bytes.fromhex(v['ephemeral_secret']),
                  nonce=bytes.fromhex(v['nonce']))
    assert sealed.hex() == v['sealed'], 'the vector does not seal to its bytes'
    assert open_drop(bytes.fromhex(v['sealed']), drop_secret, v['drive_id'],
                     v['object_key']) == (v['received'], v['folder'], raw)
    print('azlin_drop: the AZD1 vector seals and opens to its bytes')


def main(argv):
    if '--open' in argv:
        path = argv[argv.index('--open') + 1]
        secret = bytes.fromhex(argv[argv.index('--secret') + 1])
        drive = argv[argv.index('--drive') + 1]
        key = argv[argv.index('--key') + 1]
        with open(path, 'rb') as f:
            received, folder, raw = open_drop(f.read(), secret, drive, key)
        sys.stdout.write(json.dumps({'received': received, 'folder': folder}) + '\n')
        sys.stdout.buffer.write(raw)
        return 0
    if '--vector' in argv:
        check_vector()
        return 0
    self_test()
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
