#!/usr/bin/env python3
"""A paid checkout's period tokens (AZLINSEC17 F24), in Python's standard library only.

The months a checkout pays for come as blind-signed tokens: RFC 9474 RSABSSA-SHA384-PSS-Randomized,
as azlin-token's `blind-rsa-signatures` does it (one issuer key per tier and year, 2048 bits):

    message  = "azlin-period-v1:<tier>:<year>:<32 random bytes, hex>"
    prepared = randomizer[32 random bytes] || message
    m        = EMSA-PSS-ENCODE(SHA-384(prepared), bits(n) - 1)  (MGF1-SHA-384, a 48-byte salt)
    blinded  = m * r^e mod n           r random, coprime to n     -> POST /v1/tokens/issue
    z        = blinded^d mod n         the token server's blind signature
    s        = z * r^-1 mod n          an RSASSA-PSS signature of `prepared`
                                                                  -> POST /v1/drives/<id>/redeem

Blinded messages and signatures travel as standard padded base64 of as many bytes as the modulus,
the issuer key as SPKI PEM. `POST /v1/tokens/issue` takes the checkout's issue key - the one its
sealed sign-up carries as `period_tokens.issue_key` (base64url of 32 bytes); the token server keeps
only `hex(SHA-256("azlin issue key v1:" + key))`.

azcloud-kit's `period` module is the apps' side (rsa, num-bigint-dig). Here the mock token server
(scripts/azlin_mock_stack.py) signs with it and the conformance checks
(scripts/azlin_token_conformance.py) blind, finalize and verify what a token server signed:

    python3 scripts/azlin_period.py   # a blind / sign / finalize / verify round trip, and the
                                      # signature checked by openssl (when it is on the PATH)

For tests only: nothing here is constant-time, and it is no general-purpose crypto library.
"""
import base64
import hashlib
import hmac
import math
import os
import secrets
import shutil
import subprocess
import sys
import tempfile

PREFIX = 'azlin-period-v1'
HASH_LEN = 48
RANDOMIZER_LEN = 32
NONCE_LEN = 32
MAX_BLINDED = 24

# The mock token server's issuer key (made with openssl for the mock; a test key, no secret).
MOCK_N = int(
    'bee59c1b9fe8a18ebda65ca50ff01bb20ac947339a7e6e4853c1973e97cbb373'
    'fa896abacc71ac468a481d65677f987b8b53167f398abd6d1f28b02c000dfcc2'
    '5ef53c353b13675ba5a55d9856e174b2a6529ed1fbb60ca091d5d6345da8661e'
    '0016190f3d2263ac1dac92ca36d3da3514c8a37e6966139690c5e8bfe559ba8b'
    '96e34f0dc3a417605f951ef885ebb52b138cba32e868cad077f746887252b6f1'
    'ad9c74a617cbab2af1da8b3e456ed1f300d0ac705bc887b95d81b82a7df2f63b'
    'ff34174f266ac227f53e01e065f0350e7e994fe83b4f29bf214c986d812a7144'
    '27a884db21eeceab4c9a8d9fff0ba1b79b67165b69f2b8012960f128475b27ff', 16)
MOCK_E = 65537
MOCK_D = int(
    '01472dec9cc17e64347d71249c9001444362b0b993e2fb23f8170ccfbd4d4b76'
    '0ad18709c4415eb4aa5423df0af6502954eb18304c10e47dd6809988d04769d3'
    'e61c62ce3d5c9431efb711cf72306517fbe6593d4230ec87739c19a89939ecb2'
    'b28540275dcc3f3e40518aa0946bb969c4b29ec887c71daca826bdb7e03eb71c'
    '4c932f3f5177094af19dfb4979320063f409b66153106f27bb4d5504c2723eec'
    '849a4f06e3152641384ffb05087d52d02f909f307842f55ccc464c98b3a565c0'
    'dc34a5999f6018dc0f67020f4166ad7b94602094804a5635c3a6d1a05931d922'
    'ededb905d0f0012e5262a9b858e87e335e2c5a74faec3e990288a96e379af0ad', 16)


def token_message(tier, year, nonce_hex):
    return '%s:%s:%d:%s' % (PREFIX, tier, int(year), nonce_hex)


def b64(raw):
    return base64.b64encode(raw).decode('ascii')


def b64_decode(text):
    text = str(text).strip()
    return base64.b64decode(text + '=' * (-len(text) % 4), validate=True)


def _size(n):
    return (n.bit_length() + 7) // 8


# ==== The issue key ====

def new_issue_key():
    """(key, its hash): base64url of 32 random bytes, and what a checkout keeps of it."""
    key = base64.urlsafe_b64encode(secrets.token_bytes(32)).decode('ascii').rstrip('=')
    return key, issue_key_hash(key)


def issue_key_hash(key):
    return hashlib.sha256(('azlin issue key v1:' + key).encode('utf-8')).hexdigest()


def issue_key_ok(kept_hash, given):
    if not kept_hash or not given:
        return False
    return hmac.compare_digest(issue_key_hash(given), kept_hash)


# ==== SPKI PEM (RFC 5280; rsaEncryption) ====

_RSA_OID = bytes.fromhex('06092a864886f70d010101')


def _der(tag, body):
    n = len(body)
    if n < 0x80:
        head = bytes([n])
    else:
        raw = n.to_bytes((n.bit_length() + 7) // 8, 'big')
        head = bytes([0x80 | len(raw)]) + raw
    return bytes([tag]) + head + body


def _der_int(value):
    raw = value.to_bytes(value.bit_length() // 8 + 1, 'big')
    return _der(0x02, raw)


def public_key_pem(n, e):
    rsa_key = _der(0x30, _der_int(n) + _der_int(e))
    algorithm = _der(0x30, _RSA_OID + b'\x05\x00')
    spki = _der(0x30, algorithm + _der(0x03, b'\x00' + rsa_key))
    text = b64(spki)
    lines = [text[i:i + 64] for i in range(0, len(text), 64)]
    return '-----BEGIN PUBLIC KEY-----\n%s\n-----END PUBLIC KEY-----\n' % '\n'.join(lines)


def _tlv(data, at):
    tag, first = data[at], data[at + 1]
    at += 2
    if first < 0x80:
        length = first
    else:
        count = first & 0x7f
        length = int.from_bytes(data[at:at + count], 'big')
        at += count
    return tag, data[at:at + length], at + length


def parse_public_key_pem(pem):
    """(n, e) of an SPKI ("PUBLIC KEY") or PKCS#1 ("RSA PUBLIC KEY") PEM; ValueError otherwise."""
    lines = [line.strip() for line in str(pem).strip().splitlines()]
    body = ''.join(line for line in lines if line and not line.startswith('-----'))
    try:
        der = base64.b64decode(body, validate=True)
        tag, seq, _ = _tlv(der, 0)
        if tag != 0x30:
            raise ValueError('no sequence')
        tag, first, rest_at = _tlv(seq, 0)
        if tag == 0x30:
            # SPKI: the algorithm, then the key as a bit string.
            if not first.startswith(_RSA_OID):
                raise ValueError('not an RSA key')
            tag, bits, _ = _tlv(seq, rest_at)
            if tag != 0x03 or bits[:1] != b'\x00':
                raise ValueError('no key bits')
            _, seq, _ = _tlv(bits[1:], 0)
            tag, first, rest_at = _tlv(seq, 0)
        if tag != 0x02:
            raise ValueError('no modulus')
        n = int.from_bytes(first, 'big')
        tag, exponent, _ = _tlv(seq, rest_at)
        if tag != 0x02:
            raise ValueError('no exponent')
        return n, int.from_bytes(exponent, 'big')
    except (IndexError, ValueError, base64.binascii.Error) as e:
        raise ValueError('not an RSA public key PEM: %s' % e)


# ==== EMSA-PSS (RFC 8017 9.1) with SHA-384 ====

def _mgf1(seed, length):
    out = b''
    counter = 0
    while len(out) < length:
        out += hashlib.sha384(seed + counter.to_bytes(4, 'big')).digest()
        counter += 1
    return out[:length]


def emsa_pss_encode(m_hash, em_bits, salt):
    em_len = (em_bits + 7) // 8
    if em_len < HASH_LEN + len(salt) + 2:
        raise ValueError('the key is too short')
    h = hashlib.sha384(bytes(8) + m_hash + salt).digest()
    db = bytes(em_len - len(salt) - HASH_LEN - 2) + b'\x01' + salt
    masked = bytearray(a ^ b for a, b in zip(db, _mgf1(h, len(db))))
    masked[0] &= 0xff >> (8 * em_len - em_bits)
    return bytes(masked) + h + b'\xbc'


def emsa_pss_verify(m_hash, em, em_bits):
    em_len = (em_bits + 7) // 8
    if len(em) != em_len or em[-1:] != b'\xbc' or em_len < 2 * HASH_LEN + 2:
        return False
    masked, h = em[:em_len - HASH_LEN - 1], em[em_len - HASH_LEN - 1:-1]
    if masked[0] & ~(0xff >> (8 * em_len - em_bits)) & 0xff:
        return False
    db = bytearray(a ^ b for a, b in zip(masked, _mgf1(h, len(masked))))
    db[0] &= 0xff >> (8 * em_len - em_bits)
    zeros = em_len - HASH_LEN - HASH_LEN - 2
    if any(db[:zeros]) or db[zeros] != 1:
        return False
    salt = bytes(db[zeros + 1:])
    return hmac.compare_digest(hashlib.sha384(bytes(8) + m_hash + salt).digest(), h)


def _prepared_hash(randomizer, message):
    return hashlib.sha384(randomizer + message.encode('utf-8')).digest()


# ==== The client side ====

def blind(n, e, tier, year):
    """A fresh token message blinded for the key (n, e): (what finalize needs, the blinded
    message's base64)."""
    nonce = secrets.token_bytes(NONCE_LEN).hex()
    randomizer = secrets.token_bytes(RANDOMIZER_LEN)
    salt = secrets.token_bytes(HASH_LEN)
    message = token_message(tier, year, nonce)
    m = int.from_bytes(emsa_pss_encode(_prepared_hash(randomizer, message), n.bit_length() - 1,
                                       salt), 'big')
    if math.gcd(m, n) != 1:
        raise ValueError('the message shares a factor with the modulus')
    while True:
        r = secrets.randbelow(n - 1) + 1
        if math.gcd(r, n) == 1:
            break
    blinded = (m * pow(r, e, n)) % n
    state = {'tier': tier, 'year': int(year), 'nonce': nonce, 'randomizer': randomizer,
             'inverse': pow(r, -1, n)}
    return state, b64(blinded.to_bytes(_size(n), 'big'))


def finalize(n, e, state, blind_signature):
    """The token `blind_signature` (base64) finalizes into, checked; ValueError if it does not
    verify."""
    raw = b64_decode(blind_signature)
    if len(raw) != _size(n):
        raise ValueError('the blind signature has %d bytes, not %d' % (len(raw), _size(n)))
    z = int.from_bytes(raw, 'big')
    if z >= n:
        raise ValueError('the blind signature is not below the modulus')
    s = (z * state['inverse']) % n
    token = {'tier': state['tier'], 'year': state['year'], 'nonce': state['nonce'],
             'signature': b64(s.to_bytes(_size(n), 'big')), 'randomizer': b64(state['randomizer'])}
    if not verify(n, e, token):
        raise ValueError('the finalized token does not verify under the issuer key')
    return token


def verify(n, e, token):
    """Whether `token` ({tier, year, nonce, signature, randomizer}) is an RSASSA-PSS signature of
    its prepared message under (n, e)."""
    try:
        signature = b64_decode(token['signature'])
        randomizer = b64_decode(token['randomizer'])
    except (KeyError, ValueError, base64.binascii.Error):
        return False
    if len(signature) != _size(n) or len(randomizer) != RANDOMIZER_LEN:
        return False
    s = int.from_bytes(signature, 'big')
    if s >= n:
        return False
    em_bits = n.bit_length() - 1
    em = pow(s, e, n).to_bytes((em_bits + 7) // 8, 'big') if em_bits % 8 else \
        pow(s, e, n).to_bytes(_size(n), 'big')[1:]
    message = token_message(token['tier'], token['year'], token['nonce'])
    return emsa_pss_verify(_prepared_hash(randomizer, message), em, em_bits)


# ==== The issuer side (the mock token server) ====

def blind_sign(n, d, blinded):
    """The blind signature of a blinded message (base64): ValueError for one that is not as long
    as the modulus or not below it."""
    raw = b64_decode(blinded)
    if len(raw) != _size(n):
        raise ValueError('a blinded message is %d bytes, not %d' % (len(raw), _size(n)))
    z = int.from_bytes(raw, 'big')
    if z >= n:
        raise ValueError('a blinded message is not below the modulus')
    return b64(pow(z, d, n).to_bytes(_size(n), 'big'))


# ==== Self-test ====

def self_test():
    n, e, d = MOCK_N, MOCK_E, MOCK_D
    assert pow(pow(12345, e, n), d, n) == 12345, 'the mock key is an RSA key'
    pem = public_key_pem(n, e)
    assert parse_public_key_pem(pem) == (n, e), 'the PEM round trip'
    state, blinded = blind(n, e, '100GB', 2026)
    token = finalize(n, e, state, blind_sign(n, d, blinded))
    assert verify(n, e, token)
    assert token_message('100GB', 2026, token['nonce']).startswith('azlin-period-v1:100GB:2026:')
    assert not verify(n, e, dict(token, nonce='00' * 32)), 'another nonce'
    assert not verify(n, e, dict(token, tier='1TB')), 'another tier'
    other_state, other = blind(n, e, '100GB', 2026)
    assert other != blinded and other_state['nonce'] != state['nonce'], 'two blindings differ'
    try:
        finalize(n, e, other_state, blind_sign(n, d, blinded))
        raise AssertionError('a signature of another blinding finalizes')
    except ValueError:
        pass
    key, kept = new_issue_key()
    assert issue_key_ok(kept, key) and not issue_key_ok(kept, new_issue_key()[0])
    assert not issue_key_ok(kept, '') and not issue_key_ok(None, key)
    print('ok: blind / sign / finalize / verify, the PEM, the issue key')
    openssl = shutil.which('openssl')
    if not openssl:
        print('skipped: no openssl on the PATH to check the signature with')
        return
    with tempfile.TemporaryDirectory(prefix='azlin-period-') as tmp:
        paths = {name: os.path.join(tmp, name) for name in ('key.pem', 'msg', 'sig')}
        with open(paths['key.pem'], 'w') as f:
            f.write(pem)
        with open(paths['msg'], 'wb') as f:
            f.write(b64_decode(token['randomizer']) +
                    token_message(token['tier'], token['year'], token['nonce']).encode('utf-8'))
        with open(paths['sig'], 'wb') as f:
            f.write(b64_decode(token['signature']))
        checked = subprocess.run(
            [openssl, 'dgst', '-sha384', '-sigopt', 'rsa_padding_mode:pss', '-sigopt',
             'rsa_pss_saltlen:48', '-sigopt', 'rsa_mgf1_md:sha384', '-verify', paths['key.pem'],
             '-signature', paths['sig'], paths['msg']], capture_output=True, text=True)
        assert checked.returncode == 0, 'openssl: %s %s' % (checked.stdout, checked.stderr)
    print('ok: openssl verifies the finalized token as RSASSA-PSS (SHA-384, MGF1, salt 48)')


if __name__ == '__main__':
    self_test()
