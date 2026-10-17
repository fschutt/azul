#!/usr/bin/env python3
"""Ed25519 (RFC 8032) in Python's standard library only: the signatures of a drive's recovery
key, which signs a lockdown request without a drive token (`lockdown:<drive>:<nonce>`).

The mock token server (scripts/azlin_mock_stack.py) verifies them, the conformance checks
(scripts/azlin_token_conformance.py) make them; azcloud-kit's `recovery` module is the apps'
side (ed25519-dalek). The code is RFC 8032's reference implementation (section 6):

    python3 scripts/azlin_ed25519.py      # RFC 8032's test vectors 1 to 3, a sign / verify

For tests only: nothing here is constant-time, and it is no general-purpose crypto library.
"""
import base64
import hashlib
import secrets

_P = 2 ** 255 - 19
_Q = 2 ** 252 + 27742317777372353535851937790883648493


def _sha512(data):
    return hashlib.sha512(data).digest()


def _inv(x):
    return pow(x, _P - 2, _P)


_D = -121665 * _inv(121666) % _P
_SQRT_M1 = pow(2, (_P - 1) // 4, _P)


def _add(a, b):
    first = (a[1] - a[0]) * (b[1] - b[0]) % _P
    second = (a[1] + a[0]) * (b[1] + b[0]) % _P
    c = 2 * a[3] * b[3] * _D % _P
    d = 2 * a[2] * b[2] % _P
    e, f, g, h = second - first, d - c, d + c, second + first
    return (e * f, g * h, f * g, e * h)


def _mul(s, point):
    out = (0, 1, 1, 0)
    while s > 0:
        if s & 1:
            out = _add(out, point)
        point = _add(point, point)
        s >>= 1
    return out


def _equal(a, b):
    return (a[0] * b[2] - b[0] * a[2]) % _P == 0 and (a[1] * b[2] - b[1] * a[2]) % _P == 0


def _recover_x(y, sign):
    if y >= _P:
        return None
    x2 = (y * y - 1) * _inv(_D * y * y + 1)
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (_P + 3) // 8, _P)
    if (x * x - x2) % _P != 0:
        x = x * _SQRT_M1 % _P
    if (x * x - x2) % _P != 0:
        return None
    if (x & 1) != sign:
        x = _P - x
    return x


_GY = 4 * _inv(5) % _P
_GX = _recover_x(_GY, 0)
_G = (_GX, _GY, 1, _GX * _GY % _P)


def _compress(point):
    zinv = _inv(point[2])
    x = point[0] * zinv % _P
    y = point[1] * zinv % _P
    return int.to_bytes(y | ((x & 1) << 255), 32, 'little')


def _decompress(data):
    if len(data) != 32:
        return None
    y = int.from_bytes(data, 'little')
    sign = y >> 255
    y &= (1 << 255) - 1
    x = _recover_x(y, sign)
    if x is None:
        return None
    return (x, y, 1, x * y % _P)


def _expand(secret):
    h = _sha512(secret)
    a = int.from_bytes(h[:32], 'little')
    a &= (1 << 254) - 8
    a |= 1 << 254
    return a, h[32:]


def _hash_q(data):
    return int.from_bytes(_sha512(data), 'little') % _Q


def public_key(secret):
    """The 32-byte public key of a 32-byte secret (seed)."""
    a, _ = _expand(secret)
    return _compress(_mul(a, _G))


def sign(secret, message):
    a, prefix = _expand(secret)
    public = _compress(_mul(a, _G))
    r = _hash_q(prefix + message)
    rs = _compress(_mul(r, _G))
    s = (r + _hash_q(rs + public + message) * a) % _Q
    return rs + int.to_bytes(s, 32, 'little')


def verify(public, message, signature):
    if len(public) != 32 or len(signature) != 64:
        return False
    a = _decompress(public)
    r = _decompress(signature[:32])
    if a is None or r is None:
        return False
    s = int.from_bytes(signature[32:], 'little')
    if s >= _Q:
        return False
    h = _hash_q(signature[:32] + public + message)
    return _equal(_mul(s, _G), _add(r, _mul(h, a)))


def new_key():
    """(secret, public key as standard base64)."""
    secret = secrets.token_bytes(32)
    return secret, base64.b64encode(public_key(secret)).decode('ascii')


def sign_b64(secret, message):
    return base64.b64encode(sign(secret, message)).decode('ascii')


def verify_b64(public_b64, message, signature_b64):
    """Whether `signature_b64` is `public_b64`'s signature of `message` (standard base64, padding
    optional, as the token server reads them)."""
    try:
        public = base64.b64decode(public_b64 + '=' * (-len(public_b64) % 4), validate=True)
        signature = base64.b64decode(signature_b64 + '=' * (-len(signature_b64) % 4),
                                     validate=True)
    except (ValueError, TypeError):
        return False
    return verify(public, message, signature)


# RFC 8032 section 7.1, tests 1 to 3: (secret, public, message, signature), hex.
_VECTORS = [
    ('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60',
     'd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a',
     '',
     'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b'),
    ('4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb',
     '3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c',
     '72',
     '92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00'),
    ('c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7',
     'fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025',
     'af82',
     '6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a'),
]


def self_test():
    for secret, public, message, signature in _VECTORS:
        secret, public = bytes.fromhex(secret), bytes.fromhex(public)
        message, signature = bytes.fromhex(message), bytes.fromhex(signature)
        assert public_key(secret) == public, 'the public key of an RFC 8032 vector'
        assert sign(secret, message) == signature, 'the signature of an RFC 8032 vector'
        assert verify(public, message, signature)
        assert not verify(public, message + b'x', signature)
    secret, public = new_key()
    message = b'lockdown:d_1:0123456789abcdef0123456789abcdef'
    signature = sign_b64(secret, message)
    assert verify_b64(public, message, signature)
    assert not verify_b64(public, b'lockdown:d_2:0123456789abcdef0123456789abcdef', signature)
    print('ok: RFC 8032 test vectors 1 to 3, sign / verify')


if __name__ == '__main__':
    self_test()
