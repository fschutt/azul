#!/usr/bin/env python3
"""A local stand-in for the Azlin stack - the token server and the S3 balancer - for the apps' tests.

No Rust cluster, no cloud, no pip: Python's standard library only.

- S3: AzDrive's stdlib test server (examples/azul-drive/scripts/s3_server.py): SigV4 checked, path
  style, ListObjectsV2 / Get (Range) / Put / Copy / Delete / Head, one access key.
- The token server: the routes of azlin-token (azul-apps iso/crates/azlin-token, drives.rs) the apps
  use, answering the same JSON:

    POST /v1/drives {"name": ..., "tier": ...}      201 the drive bundle (a development token
                                                    server's sign-up: no payment)
    POST /v1/drives/<id>/credentials                200 a new bundle with a NEW drive token;
        Authorization: Bearer <drive token>         the old token is dead: using it again revokes
                                                    the whole family (401 token_reuse), after
                                                    which every token of it is refused (401
                                                    credentials_revoked)
    GET /health, GET /v1/health
    GET /v1/tiers                                   200 the price ladder (tiers.rs: sizes, cents
                                                    a month / a year, EUR, the methods)
    POST /v1/checkout {"tier", "months", "method"}  201 a checkout: its pay_url, its amount
    GET /v1/checkout/<id>                           200 pending | approved (with the drive
                                                    bundle as "signup", ONCE) | declined
    POST /v1/checkout/<id>/pay {"card_number"}      200 the test provider: 4242 4242 4242 4242
                                                    approves (the drive is made), others decline
    GET /v1/pay/<id>                                200 the payment page (HTML)

  Errors are {"error": "<code>", "message": "<sentence>"} with azlin-token's codes (no_such_drive,
  unauthorized, token_reuse, credentials_revoked, bad_tier, not_found). Ids look like the real
  ones: drives d_<base32>, buckets d-<base32>, tokens dt_<family>.<generation>.<random>.
  The S3 credentials are the S3 server's one key with a session token and an expiry --ttl seconds
  ahead (the real token server: 12 hours, derived per drive).

scripts/azlin_token_conformance.py runs the same HTTP checks against this server and the real one
(`azctl dev up --processes`), so the two cannot drift apart unnoticed.

Usage:

    python3 scripts/azlin_mock_stack.py [--token-port 8081] [--s3-port 9000] [--root DIR]
        [--ttl 43200] [--host 127.0.0.1]

It prints `AZLIN_MOCK_TOKEN_URL <url>` and `AZLIN_MOCK_S3_URL <url>` once both listen (port 0 picks
free ports). As a module:

    stack = azlin_mock_stack.start(root)             # free ports
    stack.token_url, stack.s3_url, stack.s3          # the s3_server.Server (requests(), ...)
    stack.stop()
"""
import argparse
import base64
import hashlib
import http.server
import json
import os
import secrets
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, os.path.join(REPO, 'examples', 'azul-drive', 'scripts'))

import s3_server  # noqa: E402

# The tiers azlin-token knows (tiers.rs): id, bytes, cents a month, cents a year; a sign-up
# without one gets the first.
TIER_LADDER = [
    ('100GB', 100_000_000_000, 99, 990),
    ('500GB', 500_000_000_000, 299, 2990),
    ('1TB', 1_000_000_000_000, 499, 4990),
    ('2TB', 2_000_000_000_000, 899, 8990),
    ('6TB', 6_000_000_000_000, 1999, 19990),
    ('12TB', 12_000_000_000_000, 3499, 34990),
]
TIERS = {tier: quota for tier, quota, _, _ in TIER_LADDER}
DEFAULT_TIER = '100GB'
# What a checkout takes (payments.rs), and the test provider's cards: the first approves, the
# second declines.
METHODS = ['sepa', 'bank_transfer', 'prepaid', 'voucher', 'app_store', 'card']
PREPAY_MONTHS = [1, 3, 6, 12, 24]
APPROVING_CARD = '4242424242424242'
DECLINING_CARD = '4000000000000002'


def tier_list():
    """GET /v1/tiers as azlin-token answers it (tiers.rs `ladder`)."""
    return {
        'tiers': [{'id': tier, 'quota_bytes': quota, 'price_cents_month': month,
                   'price_cents_year': year, 'currency': 'EUR', 'first_month_free': True,
                   'prepay_months': PREPAY_MONTHS}
                  for tier, quota, month, year in TIER_LADDER],
        'methods': METHODS,
        'legal': {
            'withdrawal_consent': 'I agree that the service starts immediately and acknowledge '
                                  'that I lose my right of withdrawal once the service has begun.',
            'sepa_prenotification': 'The amount will be debited from your account on the 1st of '
                                    'each month; the first debit follows the free month.',
            'small_amount_invoice': True,
        },
    }


def price_cents(tier, months):
    """A tier's price for `months` months (payments: the yearly price per 12 months)."""
    month, year = next((m, y) for t, _, m, y in TIER_LADDER if t == tier)
    return (months // 12) * year + (months % 12) * month
DEFAULT_TTL = 12 * 3600
REGION = s3_server.DEFAULT_REGION
ACCESS_KEY = 'AZLINMOCKKEY'
SECRET_KEY = 'azlin-mock-secret-key'


def b32(raw):
    """azlin_proto::b32: lowercase, unpadded base32."""
    return base64.b32encode(raw).decode('ascii').rstrip('=').lower()


def random_id(prefix):
    return prefix + b32(secrets.token_bytes(16))


def rfc3339(unix):
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(unix))


def token_hash(token):
    return hashlib.sha256(token.encode('utf-8')).hexdigest()


class ApiError(Exception):
    def __init__(self, status, code, message):
        super().__init__(message)
        self.status, self.code, self.message = status, code, message


class TokenState:
    """The drives and their token families, in memory (the real one keeps them in Turso)."""

    def __init__(self, s3, s3_url, ttl):
        self.s3 = s3
        self.s3_url = s3_url
        self.ttl = ttl
        self.lock = threading.Lock()
        self.drives = {}
        self.families = {}
        self.checkouts = {}
        self.base_url = ''

    def new_token(self, family):
        state = self.families[family]
        token = 'dt_%s.%d.%s' % (family, state['generation'],
                                 base64.urlsafe_b64encode(secrets.token_bytes(24))
                                 .decode('ascii').rstrip('='))
        state['current'] = token_hash(token)
        return token

    def bundle(self, drive, token):
        now = int(time.time())
        return {
            'drive': {
                'id': drive['id'],
                'name': drive['name'],
                'location': {
                    'kind': 's3',
                    'endpoint': self.s3_url,
                    'region': REGION,
                    'bucket': drive['bucket'],
                    'path_style': True,
                    'auth': {'type': 'azlin', 'drive_id': drive['id'],
                             'account_url': self.base_url},
                },
            },
            'credentials': {
                'access_key_id': ACCESS_KEY,
                'secret_access_key': SECRET_KEY,
                'session_token': 'azlin-mock-' + secrets.token_hex(12),
                'expires_at': rfc3339(now + self.ttl),
            },
            'failover': [],
            'nodes': [],
            'quota_bytes': drive['quota_bytes'],
            'read_only': False,
            'period_until': rfc3339(drive['period_until']),
            'drive_token': token,
            'tier': drive['tier'],
        }

    def signup(self, body):
        tier = str(body.get('tier') or DEFAULT_TIER).strip().upper().replace(' ', '')
        if tier not in TIERS:
            raise ApiError(400, 'bad_tier', 'unknown tier')
        name = body.get('name') or 'Azlin Storage'
        with self.lock:
            drive_id = random_id('d_')
            drive = {
                'id': drive_id,
                'bucket': 'd-' + drive_id[2:],
                'name': name,
                'tier': tier,
                'quota_bytes': TIERS[tier],
                'period_until': int(time.time()) + 30 * 86400,
            }
            self.s3.store.create_bucket(drive['bucket'])
            self.drives[drive_id] = drive
            family = random_id('f_')
            self.families[family] = {'drive': drive_id, 'generation': 0, 'current': '',
                                     'used': [], 'revoked': None}
            token = self.new_token(family)
            return self.bundle(drive, token)

    def checkout(self, body):
        """POST /v1/checkout (payments.rs `create_checkout`): a checkout to pay on its page."""
        tier = str(body.get('tier') or '').strip().upper().replace(' ', '')
        if tier not in TIERS:
            raise ApiError(400, 'bad_tier', 'unknown tier')
        method = body.get('method') or 'sepa'
        if method not in METHODS:
            raise ApiError(400, 'bad_method', 'unknown payment method')
        months = body.get('months', 1)
        if months not in PREPAY_MONTHS:
            raise ApiError(400, 'bad_months', 'prepay 1, 3, 6, 12 or 24 months')
        if method == 'bank_transfer' and months < 12:
            raise ApiError(400, 'bad_method', 'bank transfer is for yearly plans')
        amount = price_cents(tier, months)
        with self.lock:
            checkout_id = random_id('ck_')
            self.checkouts[checkout_id] = {'tier': tier, 'method': method, 'months': months,
                                           'amount': amount, 'status': 'pending', 'signup': None}
        return {'checkout_id': checkout_id,
                'pay_url': '%s/v1/pay/%s' % (self.base_url, checkout_id),
                'tier': tier, 'method': method, 'months': months, 'amount_cents': amount,
                'currency': 'EUR', 'vat_country': None, 'first_month_free': True,
                'withdrawal_consent_required': True, 'mock': True}

    def checkout_status(self, checkout_id):
        """GET /v1/checkout/<id>: pending | approved (with the drive, handed over ONCE) |
        declined."""
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            out = {'checkout_id': checkout_id, 'status': checkout['status'],
                   'tier': checkout['tier'], 'months': checkout['months'],
                   'amount_cents': checkout['amount']}
            if checkout['status'] == 'approved' and checkout['signup'] is not None:
                out['signup'] = checkout['signup']
                checkout['signup'] = None
            return out

    def pay(self, checkout_id, body):
        """POST /v1/checkout/<id>/pay (the test provider's page posts here): the approving card
        (or `prepaid`) makes the drive, anything else declines."""
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            if checkout['status'] != 'pending':
                return {'status': checkout['status']}
            tier = checkout['tier']
        card = ''.join(c for c in str(body.get('card_number') or '') if c.isdigit())
        if card == APPROVING_CARD or body.get('prepaid') is True:
            bundle = self.signup({'tier': tier, 'name': 'Azlin Storage'})
            with self.lock:
                checkout['status'] = 'approved'
                checkout['signup'] = bundle
            return {'status': 'approved'}
        with self.lock:
            checkout['status'] = 'declined'
        return {'status': 'declined',
                'reason': 'declined' if card == DECLINING_CARD else 'no payment details'}

    def refresh(self, drive_id, bearer):
        with self.lock:
            drive = self.drives.get(drive_id)
            if drive is None:
                raise ApiError(404, 'no_such_drive', 'unknown drive')
            if not bearer:
                raise ApiError(401, 'unauthorized', 'a drive token is required')
            if not bearer.startswith('dt_'):
                raise ApiError(401, 'unauthorized', 'malformed drive token')
            family = bearer[3:].split('.')[0]
            state = self.families.get(family)
            if state is None or state['drive'] != drive_id:
                raise ApiError(401, 'unauthorized', 'unknown token')
            if state['revoked']:
                raise ApiError(401, 'credentials_revoked', 'this device was removed from the drive')
            digest = token_hash(bearer)
            if digest == state['current']:
                state['used'] = (state['used'] + [state['current']])[-20:]
                state['generation'] += 1
                return self.bundle(drive, self.new_token(family))
            if digest in state['used']:
                # A rotated token used again: it leaked. The whole family is revoked.
                state['revoked'] = 'reuse'
                raise ApiError(401, 'token_reuse',
                               'an old token was reused: the device must sign in again')
            raise ApiError(401, 'unauthorized', 'unknown token')


class TokenHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    server_version = 'azlin-token-mock/1'

    def log_message(self, fmt, *args):
        if self.server.verbose:
            sys.stderr.write('[azlin-token-mock] %s\n' % (fmt % args))

    def answer(self, status, value):
        body = json.dumps(value).encode('utf-8')
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.server.record({'method': self.command, 'path': self.path, 'status': status})

    def body(self):
        length = int(self.headers.get('Content-Length') or 0)
        raw = self.rfile.read(length) if length else b''
        if not raw:
            return {}
        try:
            value = json.loads(raw.decode('utf-8'))
        except ValueError:
            return None
        return value if isinstance(value, dict) else None

    def bearer(self):
        value = self.headers.get('Authorization') or ''
        for prefix in ('Bearer ', 'bearer '):
            if value.startswith(prefix):
                return value[len(prefix):].strip()
        return None

    def route(self):
        state = self.server.state
        path = self.path.split('?', 1)[0].strip('/')
        segments = path.split('/') if path else []
        if self.command == 'GET' and segments in (['health'], ['v1', 'health']):
            body = b'ok\n'
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.command == 'GET' and segments == ['v1', 'tiers']:
            self.answer(200, tier_list())
            return
        if self.command == 'POST' and segments == ['v1', 'drives']:
            body = self.body()
            self.answer(201, state.signup(body or {}))
            return
        if self.command == 'POST' and segments == ['v1', 'checkout']:
            self.answer(201, state.checkout(self.body() or {}))
            return
        if self.command == 'GET' and len(segments) == 3 and segments[:2] == ['v1', 'checkout']:
            self.answer(200, state.checkout_status(segments[2]))
            return
        if self.command == 'POST' and len(segments) == 4 and segments[:2] == ['v1', 'checkout'] \
                and segments[3] == 'pay':
            self.answer(200, state.pay(segments[2], self.body() or {}))
            return
        if self.command == 'GET' and len(segments) == 3 and segments[:2] == ['v1', 'pay']:
            page = ('<!doctype html><title>Azlin mock payment</title><h1>Mock payment for '
                    'checkout %s</h1><p>POST JSON to /v1/checkout/%s/pay: {"card_number": '
                    '"4242 4242 4242 4242"} approves.</p>' % (segments[2], segments[2]))
            body = page.encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            self.server.record({'method': self.command, 'path': self.path, 'status': 200})
            return
        if self.command == 'POST' and len(segments) == 4 and segments[:2] == ['v1', 'drives'] \
                and segments[3] == 'credentials':
            self.body()
            self.answer(200, state.refresh(segments[2], self.bearer()))
            return
        raise ApiError(404, 'not_found', 'no route for %s /%s' % (self.command, path))

    def handle_any(self):
        try:
            self.route()
        except ApiError as e:
            self.answer(e.status, {'error': e.code, 'message': e.message})

    do_GET = handle_any
    do_POST = handle_any


class TokenServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address, state, verbose=False):
        super().__init__(address, TokenHandler)
        self.state = state
        self.verbose = verbose
        self._log = []
        self._log_lock = threading.Lock()
        self._thread = None

    @property
    def url(self):
        host, port = self.server_address[:2]
        return 'http://%s:%d' % (host, port)

    def record(self, entry):
        with self._log_lock:
            self._log.append(entry)

    def requests(self):
        with self._log_lock:
            return list(self._log)

    def start_background(self):
        self._thread = threading.Thread(target=self.serve_forever, name='azlin-token-mock',
                                        daemon=True)
        self._thread.start()
        return self

    def stop(self):
        self.shutdown()
        self.server_close()
        if self._thread:
            self._thread.join(timeout=5)


class Stack:
    """The two servers of one mock stack."""

    def __init__(self, token, s3):
        self.token = token
        self.s3 = s3

    @property
    def token_url(self):
        return self.token.url

    @property
    def s3_url(self):
        return self.s3.url

    def stop(self):
        self.token.stop()
        self.s3.stop()


def start(root, host='127.0.0.1', token_port=0, s3_port=0, ttl=DEFAULT_TTL, verbose=False):
    """Both servers on `host` (port 0: a free one), serving in background threads; the S3 objects
    live under `root/<bucket>/<key>`."""
    s3 = s3_server.start(root, host=host, port=s3_port, access_key=ACCESS_KEY,
                         secret_key=SECRET_KEY, region=REGION, verbose=verbose)
    state = TokenState(s3, s3.url, ttl)
    token = TokenServer((host, token_port), state, verbose).start_background()
    state.base_url = token.url
    return Stack(token, s3)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--host', default='127.0.0.1')
    parser.add_argument('--token-port', type=int, default=0,
                        help='the token server (azctl dev up: 8081; default: a free port)')
    parser.add_argument('--s3-port', type=int, default=0,
                        help='the S3 balancer (azctl dev up: 9000; default: a free port)')
    parser.add_argument('--root', help='where the S3 objects live (default: a new temporary folder)')
    parser.add_argument('--ttl', type=int, default=DEFAULT_TTL,
                        help='seconds the S3 credentials of a bundle are valid (default 12 h)')
    parser.add_argument('--verbose', action='store_true')
    args = parser.parse_args(argv)
    root = args.root or tempfile.mkdtemp(prefix='azlin-mock-s3-')
    stack = start(root, args.host, args.token_port, args.s3_port, args.ttl, args.verbose)
    print('AZLIN_MOCK_TOKEN_URL %s' % stack.token_url, flush=True)
    print('AZLIN_MOCK_S3_URL %s' % stack.s3_url, flush=True)
    print('[azlin-mock] token server %s, S3 %s, objects in %s' % (stack.token_url, stack.s3_url,
                                                                    root), file=sys.stderr)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass
    finally:
        stack.stop()


if __name__ == '__main__':
    main()
