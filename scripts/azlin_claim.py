#!/usr/bin/env python3
"""The claim of a paid Azlin drive (CLAIM CONTRACT v1), in Python's standard library only.

A checkout names the X25519 public key the app made for it (`claim_key`); once the payment is
approved the token server hands the drive's sign-up out SEALED to that key, so only the app that
started the checkout can open it - whoever else learns the checkout id (the payment page's
address) learns nothing:

    eph    = a fresh X25519 secret
    shared = X25519(eph, claim_pk)
    key    = HKDF-SHA256(ikm = shared, salt = eph_pk || claim_pk, info = b"azlin-claim-v1"), 32 bytes
    nonce  = 12 random bytes
    ct     = ChaCha20-Poly1305(key, nonce, plaintext = the sign-up JSON's UTF-8 bytes,
                               aad = the checkout id's UTF-8 bytes)
    sealed = standard padded base64(eph_pk[32] || nonce[12] || ct)

azcloud-kit's `claim` module opens it in the apps (x25519-dalek, hkdf, chacha20poly1305). Here
the mock token server (scripts/azlin_mock_stack.py) seals with it and the conformance checks
(scripts/azlin_token_conformance.py) open what a token server sealed. No pip package: the test
scripts run on the stock python3 of a CI runner, so X25519 (RFC 7748), HKDF (RFC 5869) and
ChaCha20-Poly1305 (RFC 8439) are written out below and checked against the RFCs' test vectors:

    python3 scripts/azlin_claim.py            # the RFC vectors and a seal / open round trip
    python3 scripts/azlin_claim.py --vector   # the fixed vector azcloud-kit's tests open

For tests only: nothing here is constant-time, and it is no general-purpose crypto library.
"""
import base64
import hashlib
import hmac
import json
import secrets
import struct
import sys

# The HKDF info of the contract.
INFO = b'azlin-claim-v1'
KEY_LEN = 32
NONCE_LEN = 12
TAG_LEN = 16

# ==== X25519 (RFC 7748) ====

_P = 2 ** 255 - 19
_A24 = 121665
BASE_POINT = bytes([9]) + bytes(31)


def _scalar(k):
    b = bytearray(k)
    b[0] &= 248
    b[31] &= 127
    b[31] |= 64
    return int.from_bytes(b, 'little')


def _u_coordinate(u):
    b = bytearray(u)
    b[31] &= 127
    return int.from_bytes(b, 'little')


def x25519(k, u):
    """X25519(k, u) of two 32-byte strings (RFC 7748 section 5, the Montgomery ladder)."""
    if len(k) != 32 or len(u) != 32:
        raise ValueError('X25519 takes 32-byte scalars and u-coordinates')
    x1 = _u_coordinate(u)
    k = _scalar(k)
    x2, z2, x3, z3 = 1, 0, x1, 1
    swap = 0
    for t in reversed(range(255)):
        bit = (k >> t) & 1
        swap ^= bit
        if swap:
            x2, x3, z2, z3 = x3, x2, z3, z2
        swap = bit
        a = (x2 + z2) % _P
        aa = a * a % _P
        b = (x2 - z2) % _P
        bb = b * b % _P
        e = (aa - bb) % _P
        c = (x3 + z3) % _P
        d = (x3 - z3) % _P
        da = d * a % _P
        cb = c * b % _P
        x3 = (da + cb) * (da + cb) % _P
        z3 = x1 * (da - cb) * (da - cb) % _P
        x2 = aa * bb % _P
        z2 = e * (aa + _A24 * e) % _P
    if swap:
        x2, x3, z2, z3 = x3, x2, z3, z2
    return (x2 * pow(z2, _P - 2, _P) % _P).to_bytes(32, 'little')


def public_key(secret):
    """The X25519 public key of a 32-byte secret."""
    return x25519(secret, BASE_POINT)


# ==== HKDF-SHA256 (RFC 5869) ====

def hkdf_sha256(ikm, salt, info, length=KEY_LEN):
    prk = hmac.new(salt or bytes(32), ikm, hashlib.sha256).digest()
    okm, block, counter = b'', b'', 1
    while len(okm) < length:
        block = hmac.new(prk, block + info + bytes([counter]), hashlib.sha256).digest()
        okm += block
        counter += 1
    return okm[:length]


# ==== ChaCha20-Poly1305 (RFC 8439) ====

def _rotl(v, c):
    return ((v << c) & 0xffffffff) | (v >> (32 - c))


def _quarter(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & 0xffffffff
    s[d] = _rotl(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & 0xffffffff
    s[b] = _rotl(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & 0xffffffff
    s[d] = _rotl(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & 0xffffffff
    s[b] = _rotl(s[b] ^ s[c], 7)


def chacha20_block(key, counter, nonce):
    state = [0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]
    state += list(struct.unpack('<8I', key)) + [counter] + list(struct.unpack('<3I', nonce))
    w = list(state)
    for _ in range(10):
        _quarter(w, 0, 4, 8, 12)
        _quarter(w, 1, 5, 9, 13)
        _quarter(w, 2, 6, 10, 14)
        _quarter(w, 3, 7, 11, 15)
        _quarter(w, 0, 5, 10, 15)
        _quarter(w, 1, 6, 11, 12)
        _quarter(w, 2, 7, 8, 13)
        _quarter(w, 3, 4, 9, 14)
    return struct.pack('<16I', *((w[i] + state[i]) & 0xffffffff for i in range(16)))


def chacha20_xor(key, counter, nonce, data):
    out = bytearray()
    for offset in range(0, len(data), 64):
        block = chacha20_block(key, counter + offset // 64, nonce)
        out += bytes(a ^ b for a, b in zip(data[offset:offset + 64], block))
    return bytes(out)


def poly1305(key, message):
    r = int.from_bytes(key[:16], 'little') & 0x0ffffffc0ffffffc0ffffffc0fffffff
    s = int.from_bytes(key[16:32], 'little')
    p = (1 << 130) - 5
    acc = 0
    for offset in range(0, len(message), 16):
        n = int.from_bytes(message[offset:offset + 16] + b'\x01', 'little')
        acc = (acc + n) * r % p
    return ((acc + s) & ((1 << 128) - 1)).to_bytes(16, 'little')


def _pad16(data):
    return bytes(-len(data) % 16)


def _tag(key, nonce, ciphertext, aad):
    one_time_key = chacha20_block(key, 0, nonce)[:32]
    mac_data = (aad + _pad16(aad) + ciphertext + _pad16(ciphertext)
                + struct.pack('<QQ', len(aad), len(ciphertext)))
    return poly1305(one_time_key, mac_data)


def aead_encrypt(key, nonce, plaintext, aad):
    """ChaCha20-Poly1305: the ciphertext with its 16-byte tag."""
    ciphertext = chacha20_xor(key, 1, nonce, plaintext)
    return ciphertext + _tag(key, nonce, ciphertext, aad)


def aead_decrypt(key, nonce, sealed, aad):
    """The plaintext of `sealed` (ciphertext and tag); ValueError when it does not open."""
    if len(sealed) < TAG_LEN:
        raise ValueError('too short to hold a tag')
    ciphertext, tag = sealed[:-TAG_LEN], sealed[-TAG_LEN:]
    if not hmac.compare_digest(_tag(key, nonce, ciphertext, aad), tag):
        raise ValueError('it does not open with this key, nonce and checkout id')
    return chacha20_xor(key, 1, nonce, ciphertext)


# ==== The claim ====

def claim_key_bytes(claim_key):
    """The 32 bytes of a checkout's `claim_key` (standard padded base64); ValueError for
    anything else - and for a key of a small order, which would make every seal readable."""
    try:
        raw = base64.b64decode(claim_key.encode('ascii') if isinstance(claim_key, str)
                               else claim_key, validate=True)
    except (ValueError, UnicodeEncodeError) as e:
        raise ValueError('the claim key is not standard base64: %s' % e)
    if len(raw) != KEY_LEN or base64.b64encode(raw).decode('ascii') != str(claim_key):
        raise ValueError('the claim key is not the padded base64 of 32 bytes')
    if x25519(bytes([1]) + bytes(30) + bytes([64]), raw) == bytes(32):
        raise ValueError('the claim key is a point of a small order')
    return raw


def _sealing_key(shared, eph_pk, claim_pk):
    return hkdf_sha256(shared, eph_pk + claim_pk, INFO, KEY_LEN)


def seal(plaintext, claim_key, checkout_id, eph_secret=None, nonce=None):
    """`plaintext` (the sign-up JSON's bytes) sealed to `claim_key` (base64) for `checkout_id`;
    `eph_secret` and `nonce` are fresh random ones unless a test fixes them."""
    claim_pk = claim_key_bytes(claim_key)
    eph_secret = eph_secret if eph_secret is not None else secrets.token_bytes(KEY_LEN)
    nonce = nonce if nonce is not None else secrets.token_bytes(NONCE_LEN)
    eph_pk = public_key(eph_secret)
    shared = x25519(eph_secret, claim_pk)
    if shared == bytes(32):
        raise ValueError('the claim key gives no shared secret')
    key = _sealing_key(shared, eph_pk, claim_pk)
    sealed = aead_encrypt(key, nonce, plaintext, checkout_id.encode('utf-8'))
    return base64.b64encode(eph_pk + nonce + sealed).decode('ascii')


def open_sealed(sealed, claim_secret, checkout_id):
    """The plaintext of a sealed sign-up, opened with the app's claim secret (32 bytes);
    ValueError when it does not open (another key, another checkout id, a changed byte)."""
    raw = base64.b64decode(sealed.encode('ascii'), validate=True)
    if len(raw) < KEY_LEN + NONCE_LEN + TAG_LEN:
        raise ValueError('too short to be a sealed sign-up')
    eph_pk, nonce, body = raw[:KEY_LEN], raw[KEY_LEN:KEY_LEN + NONCE_LEN], raw[KEY_LEN + NONCE_LEN:]
    shared = x25519(claim_secret, eph_pk)
    if shared == bytes(32):
        raise ValueError('the sealed sign-up names a point of a small order')
    key = _sealing_key(shared, eph_pk, public_key(claim_secret))
    return aead_decrypt(key, nonce, body, checkout_id.encode('utf-8'))


def new_claim_key():
    """A claim secret and its public key as a checkout names it: (secret bytes, base64 key)."""
    secret = secrets.token_bytes(KEY_LEN)
    return secret, base64.b64encode(public_key(secret)).decode('ascii')


# ==== The checks ====

def _hex(text):
    return bytes.fromhex(text.replace(' ', '').replace('\n', ''))


def self_test():
    """The RFCs' test vectors and a round trip; AssertionError names the one that failed."""
    # RFC 7748 section 5.2.
    assert x25519(
        _hex('a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4'),
        _hex('e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c'),
    ) == _hex('c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552'), 'RFC 7748 5.2 #1'
    assert x25519(
        _hex('4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d'),
        _hex('e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493'),
    ) == _hex('95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957'), 'RFC 7748 5.2 #2'
    assert x25519(BASE_POINT, BASE_POINT) == _hex(
        '422c8e7a6227d7bca1350b3e2bb7279f7897b87bb6854b783c60e80311ae3079'), 'RFC 7748 5.2 iterated'
    # RFC 7748 section 6.1: Alice and Bob.
    alice = _hex('77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a')
    bob = _hex('5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb')
    assert public_key(alice) == _hex(
        '8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a'), 'RFC 7748 6.1 Alice'
    assert public_key(bob) == _hex(
        'de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f'), 'RFC 7748 6.1 Bob'
    shared = _hex('4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742')
    assert x25519(alice, public_key(bob)) == shared, 'RFC 7748 6.1 shared (Alice)'
    assert x25519(bob, public_key(alice)) == shared, 'RFC 7748 6.1 shared (Bob)'
    # RFC 5869 A.1.
    assert hkdf_sha256(bytes([0x0b] * 22), _hex('000102030405060708090a0b0c'),
                       _hex('f0f1f2f3f4f5f6f7f8f9'), 42) == _hex(
        '3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865'
    ), 'RFC 5869 A.1'
    # RFC 8439 section 2.8.2.
    key = bytes(range(0x80, 0xa0))
    nonce = _hex('070000004041424344454647')
    aad = _hex('50515253c0c1c2c3c4c5c6c7')
    plaintext = (b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip "
                 b"for the future, sunscreen would be it.")
    expected = _hex(
        'd31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69'
        'da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad67594'
        '5585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116'
        '1ae10b594f09e26a7e902ecbd0600691')
    assert aead_encrypt(key, nonce, plaintext, aad) == expected, 'RFC 8439 2.8.2 seal'
    assert aead_decrypt(key, nonce, expected, aad) == plaintext, 'RFC 8439 2.8.2 open'
    # A round trip, and what must not open.
    secret, claim = new_claim_key()
    sealed = seal(b'{"drive": {}}', claim, 'ck_1')
    assert open_sealed(sealed, secret, 'ck_1') == b'{"drive": {}}', 'round trip'
    for wrong in ((secrets.token_bytes(32), 'ck_1'), (secret, 'ck_2')):
        try:
            open_sealed(sealed, *wrong)
            raise AssertionError('opened with a wrong key or checkout id')
        except ValueError:
            pass
    raw = bytearray(base64.b64decode(sealed))
    raw[-1] ^= 1
    try:
        open_sealed(base64.b64encode(bytes(raw)).decode('ascii'), secret, 'ck_1')
        raise AssertionError('opened with a flipped byte')
    except ValueError:
        pass
    for bad in ('', 'AAAA', base64.b64encode(bytes(32)).decode('ascii')):
        try:
            claim_key_bytes(bad)
            raise AssertionError('took the claim key %r' % bad)
        except ValueError:
            pass


# The fixed vector azcloud-kit's tests open (`a_signup_the_mock_token_server_sealed_opens`):
# the claim secret is the bytes 1..=32, the ephemeral secret 101..=132, the nonce 201..=212.
VECTOR_CHECKOUT = 'ck_vector'
VECTOR_SIGNUP = {'drive_token': 'dt_f_vector.0.claimed', 'drive': {'id': 'd_vector'}}


def vector():
    claim_secret = bytes(range(1, 33))
    claim = base64.b64encode(public_key(claim_secret)).decode('ascii')
    plaintext = json.dumps(VECTOR_SIGNUP, separators=(',', ':')).encode('utf-8')
    sealed = seal(plaintext, claim, VECTOR_CHECKOUT, bytes(range(101, 133)),
                  bytes(range(201, 213)))
    return {
        'claim_secret': base64.b64encode(claim_secret).decode('ascii'),
        'claim_key': claim,
        'checkout_id': VECTOR_CHECKOUT,
        'plaintext': plaintext.decode('utf-8'),
        'sealed': sealed,
    }


if __name__ == '__main__':
    self_test()
    if '--vector' in sys.argv[1:]:
        print(json.dumps(vector(), indent=2))
    else:
        print('ok: X25519 (RFC 7748), HKDF-SHA256 (RFC 5869), ChaCha20-Poly1305 (RFC 8439) and '
              'the claim round trip')
