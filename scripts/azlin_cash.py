#!/usr/bin/env python3
"""Cash by post (cash contract v1): the activation code of a cash checkout, in Python's standard
library only.

    POST /v1/checkout {"tier", "months", "method": "cash", "claim_key"}
    201 {"checkout_id", "status": "awaiting_cash", "amount_cents", "currency",
         "activation_code", "mail_to": {"name", "lines": [...]}, "expires_at"}

The activation code goes on the slip the buyer posts with the cash; the operator's AzCtl reads it
back and activates the checkout (`GET /v1/checkout/<id>` answers `approved` then, and the drive
follows by the claim):

    raw  = checkout_id (UTF-8) || amount_cents (u32, big endian) || currency (3 ASCII letters)
    code = "AZC1-" + base32(raw || HMAC-SHA256(the server's cash key, raw)[:10])

base32 is RFC 4648's, upper case, without padding, in blocks of four joined by `-`. The mock token
server (scripts/azlin_mock_stack.py) makes it with its test key; the conformance checks
(scripts/azlin_token_conformance.py) read it back and check the MAC with the mock's key (or one
given for a real server); azcloud-kit's `cash::ActivationCode` reads it in the apps.

    python3 scripts/azlin_cash.py     # the vector azul-pay's and azcloud-kit's tests use
"""
import base64
import hashlib
import hmac
import re
import struct
import sys

PREFIX = 'AZC1-'
MAC_LEN = 10
# A code as the token server writes it.
SHAPE = re.compile(r'^AZC1-(?:[A-Z2-7]{4}-)*[A-Z2-7]{1,4}$')
# The mock token server's cash key: a test key, public on purpose.
MOCK_KEY = b'azlin-mock-cash-key'


def _b32(raw):
    return base64.b32encode(raw).decode('ascii').rstrip('=')


def _grouped(text):
    return '-'.join(text[i:i + 4] for i in range(0, len(text), 4))


def _signed(checkout_id, amount_cents, currency):
    return checkout_id.encode('utf-8') + struct.pack('>I', amount_cents) + currency.encode('ascii')


def activation_code(checkout_id, amount_cents, currency, key):
    """The activation code of `checkout_id` for `amount_cents` of `currency`, MAC'd with `key`."""
    raw = _signed(checkout_id, amount_cents, currency)
    mac = hmac.new(key, raw, hashlib.sha256).digest()[:MAC_LEN]
    return PREFIX + _grouped(_b32(raw + mac))


def parse_activation_code(text):
    """{"checkout_id", "amount_cents", "currency", "mac", "signed"} of an activation code as
    typed (any case, blanks and dashes do not matter); ValueError for anything else."""
    compact = ''.join(c for c in text if not c.isspace() and c != '-').upper()
    if not compact.startswith('AZC1'):
        raise ValueError('not an activation code (AZC1-...)')
    body = compact[4:]
    try:
        raw = base64.b32decode(body + '=' * (-len(body) % 8))
    except ValueError:
        raise ValueError('not an activation code: no base32') from None
    if _b32(raw) != body or len(raw) <= 4 + 3 + MAC_LEN:
        raise ValueError('not an activation code: its length')
    ident, rest = raw[:-(4 + 3 + MAC_LEN)], raw[-(4 + 3 + MAC_LEN):]
    return {'checkout_id': ident.decode('utf-8'),
            'amount_cents': struct.unpack('>I', rest[:4])[0],
            'currency': rest[4:7].decode('ascii'),
            'mac': rest[7:],
            'signed': raw[:-MAC_LEN]}


def verify(parsed, key):
    """Whether the parsed code's MAC is `key`'s (the operator's check)."""
    mac = hmac.new(key, parsed['signed'], hashlib.sha256).digest()[:MAC_LEN]
    return hmac.compare_digest(mac, parsed['mac'])


# The token server's test vector (azlin-token cash.rs, SRV17): the id bytes 00 01 .. 0f, EUR 11.88.
VECTOR = ('ck_aaaqeayeaudaocajbifqydiob4', 1188, 'EUR', b'cash-key-for-tests',
          'AZC1-AAAQ-EAYE-AUDA-OCAJ-BIFQ-YDIO-B4AA-ABFE-IVKV-F3QG-5NDF-5KZK-SMTY-I')


def self_test():
    checkout_id, cents, currency, key, want = VECTOR
    code = activation_code(checkout_id, cents, currency, key)
    assert code == want, code
    assert SHAPE.match(code) and len(code.replace('-', '')) == 4 + 53
    parsed = parse_activation_code(code.lower())
    assert (parsed['checkout_id'], parsed['amount_cents'], parsed['currency']) == \
        (checkout_id, cents, currency), parsed
    assert verify(parsed, key) and not verify(parsed, b'another key')
    forged = parse_activation_code(activation_code(checkout_id, 99, 'EUR', b'another key'))
    assert not verify(forged, key), 'another amount needs the key'
    try:
        parse_activation_code('AZC1-MNVV-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYIA-'
                              'AAB5-4RKV-KI74-IMPG-BG5O-LTW7-WE')
        raise AssertionError('a code with the id as ASCII was read')
    except ValueError:
        pass
    return code


if __name__ == '__main__':
    print(self_test())
    sys.exit(0)
