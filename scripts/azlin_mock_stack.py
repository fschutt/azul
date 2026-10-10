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
    POST /v1/checkout {"tier", "months", "method",  201 a checkout: its pay_url, its amount
                       "claim_key"}                 (400 claim_key_required without the key)
    GET /v1/checkout/<id>                           200 pending | approved (the drive bundle
                                                    SEALED to the claim key as "sealed_signup",
                                                    to every poll for 30 days) | declined |
                                                    expired
    POST /v1/checkout/<id>/pay {"card_number"}      200 the test provider: 4242 4242 4242 4242
                                                    approves (the drive is made), others decline
    GET /v1/pay/<id>                                200 the payment page (HTML)
    GET /v1/tokens/keys                             200 the period tokens' issuer keys: one per
                                                    tier, of this year (SPKI PEM)
    POST /v1/tokens/issue {"checkout_id",           200 one blind signature per blinded message,
        "issue_key", "key_id",                      up to the checkout's months in all (400
        "blinded": ["<base64>"]}                    issue_key_required / key_id_required, 403
                                                    issue_key_wrong, 404 no_such_checkout, 409
                                                    not_paid / already_issued; 409 key_changed
                                                    with the current key_id and its key for a
                                                    key other than this or last year's, nothing
                                                    counted); the identical request again (same
                                                    key_id, messages, order) the same answer,
                                                    counted once
    POST /v1/drives/<id>/redeem {"tier", "year",    200 the drive's period a month longer (400
        "nonce", "signature", "randomizer"}         bad_token / wrong_tier, 409 token_used); the
        Authorization: Bearer <drive token>         drive token is checked, not spent - the one
                                                    the family rotated from last counts too
    GET /v1/drives/<id>                             200 the drive's tier, quota and period (a read:
        Authorization: Bearer <drive token>         the previous token counts too; an older one
                                                    is a reuse)

  With fake payment providers (`--providers`, `set_providers`; CHECKOUT-PLAN §3.11, §4.2 - see
  "The fake payment providers" below for their pages and webhooks):

    GET /v1/checkout/options?tier&months&country    200 the offered fakes' descriptors, the price
        &currency&surfaces                          with its VAT, the legal texts (404 without
                                                    providers: the v1 checkout)
    POST /v1/checkout {..., "provider", "method",   201 + provider, method, surface (a fields page
                       "surface", "vat_country",    with its client secret, a hosted page, a page
                       "withdrawal_consent"}        for the browser), return pages, expires_at
    POST /v1/checkout/<id>/surface {"kind"}         200 the same checkout on another surface
    POST /v1/checkout/<id>/abandon                  200 the provider session expires
    POST /v1/webhook/<provider>                     200 a signed provider webhook (each event once)
    GET /fields/fake-stripe/v1, /fake-*/...,        the fakes' pages; /return/ok|cancel|pending;
        /_bridge/...                                the bridge (204)

  The claim (CLAIM CONTRACT v1, scripts/azlin_claim.py): a checkout names the standard padded
  base64 of the X25519 public key the app made for it; the approved checkout's sign-up is sealed
  to it (X25519 + HKDF-SHA256 "azlin-claim-v1" + ChaCha20-Poly1305, the checkout id as associated
  data), so only that app opens it - however late it asks, from another process, after a
  restart. The plaintext `signup` of earlier token servers is gone. The sealed sign-up carries
  `period_tokens: {"checkout_id", "months", "issue_key"}` (AZLINSEC17 F24): the checkout's months
  as blind-signed period tokens (RFC 9474, scripts/azlin_period.py), issued only against that key
  (the mock keeps its hash) - the checkout id alone issues nothing.

  Errors are {"error": "<code>", "message": "<sentence>"} with azlin-token's codes (no_such_drive,
  unauthorized, token_reuse, credentials_revoked, bad_tier, no_such_checkout, claim_key_required,
  bad_claim_key, issue_key_required, issue_key_wrong, not_paid, already_issued, bad_token,
  wrong_tier, token_used, not_found). Ids look like the real ones: drives d_<base32>, buckets d-<base32>,
  tokens dt_<family>.<generation>.<random>. The S3 credentials are the S3 server's one key with
  a session token and an expiry --ttl seconds ahead (the real token server: 12 hours, derived per
  drive).

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
import hmac
import html
import http.server
import json
import os
import secrets
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, os.path.join(REPO, 'examples', 'azul-drive', 'scripts'))
sys.path.insert(0, HERE)

import azlin_claim  # noqa: E402
import azlin_period  # noqa: E402
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
# How long an approved checkout keeps its sealed sign-up (then it answers "expired").
SEALED_KEEP_SECS = 30 * 86400

# ==== The fake payment providers (CHECKOUT-PLAN §4.2) ====
#
# One fake per provider shape, with the shape's pages, redirects and signed webhooks - not its
# look. They exist only when the stack is started with `--providers` (or after
# `stack.token.state.set_providers([...])`): without them the token server has no payment
# options (GET /v1/checkout/options answers 404) and the apps pay on the v1 page. The apps'
# registry (azul-pay, feature `fake-providers`) knows the same ids, and takes them only from a
# token server on this computer.
#
#   fake-stripe      card: our fields page /fields/fake-stripe/v1 (plain inputs taking Stripe's
#                    test cards, talking to the app through /_bridge/ navigations), a hosted
#                    page /fake-stripe/c/pay/cs_test_<ref>, the browser; webhooks signed with
#                    Stripe's scheme (Stripe-Signature: t=..,v1=HMAC-SHA256(secret, "t.body"))
#   fake-gocardless  SEPA Direct Debit: a Billing Request Flow page /fake-gocardless/flow/BRQ<ref>
#                    taking a test IBAN; webhooks with Webhook-Signature (HMAC-SHA256 of the body)
#   fake-paypal      PayPal: a login + approve page on "localhost" (another host than the other
#                    fakes': the system browser only); webhooks with x-provider-signature
#   fake-mor         a merchant of record's hosted checkout /fake-mor/checkout/<ref> (opt-in)
#
# The return pages /return/ok, /return/cancel, /return/pending and the bridge /_bridge/... live
# on the token server's host (pay.azlin.io's stand-in). A provider only ever sees the
# checkout's random `provider_ref` (pr_...), never the checkout id.
FAKE_PROVIDERS = ('fake-stripe', 'fake-gocardless', 'fake-paypal', 'fake-mor')
# The providers a stack started with `--providers` without a list offers.
DEFAULT_PROVIDERS = ('fake-stripe', 'fake-gocardless', 'fake-paypal')
# Each fake's methods and their surfaces, best first.
PROVIDER_METHODS = {
    'fake-stripe': {'card': ['fields', 'page', 'browser']},
    'fake-gocardless': {'sepa_debit': ['page', 'browser']},
    'fake-paypal': {'paypal': ['browser']},
    'fake-mor': {'card': ['page', 'browser']},
}
PROVIDER_KINDS = {'fake-mor': 'merchant_of_record'}
# The local webhook secrets (no real provider's).
WEBHOOK_SECRETS = {
    'fake-stripe': 'whsec_local_fake_stripe',
    'fake-gocardless': 'local-fake-gocardless-webhook-secret',
    'fake-paypal': 'local-fake-paypal-webhook-secret',
    'fake-mor': 'local-fake-mor-webhook-secret',
}
# Stripe's libraries' default tolerance for a webhook's timestamp.
STRIPE_TOLERANCE_SECS = 300
# The test IBANs of the fake GoCardless: the first is mandated, the second fails.
APPROVING_IBAN = 'DE89370400440532013000'
DECLINING_IBAN = 'DE62370400440532013001'
# Where SEPA Direct Debit is offered (the fake GoCardless's countries).
SEPA_COUNTRIES = {
    'AT', 'BE', 'BG', 'CY', 'CZ', 'DE', 'DK', 'EE', 'ES', 'FI', 'FR', 'GR', 'HR', 'HU', 'IE',
    'IT', 'LT', 'LU', 'LV', 'MT', 'NL', 'PL', 'PT', 'RO', 'SE', 'SI', 'SK', 'IS', 'LI', 'NO',
    'CH', 'GB', 'MC', 'SM', 'VA', 'AD',
}
VAT_PERMILLE = 190

# The fake Stripe's fields page: plain inputs, no script but its own; the inputs (publishable
# key, client secret, locale, look) come in the fragment; it tells the app through main-frame
# navigations to /_bridge/<message> (which the app cancels) and hears the app's commands as new
# fragments (hashchange).
FIELDS_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Fake Stripe card fields</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'">
<style>
body { margin: 0; padding: 8px; font: 14px system-ui, sans-serif; background: transparent; }
input { box-sizing: border-box; width: 100%; margin: 0 0 8px 0; padding: 8px; font: inherit;
        border: 1px solid #b8b8b8; border-radius: 4px; }
.row { display: flex; gap: 8px; }
</style></head>
<body>
<input id="number" placeholder="Card number (4242 4242 4242 4242)" autocomplete="cc-number">
<div class="row"><input id="exp" placeholder="MM / YY"><input id="cvc" placeholder="CVC"></div>
<script>
(function () {
  var inputs = new URLSearchParams(location.hash.slice(1));
  var secret = inputs.get('cs');
  var queue = [];
  function flush() {
    if (!queue.length) { return; }
    location.href = queue.shift();
    if (queue.length) { setTimeout(flush, 150); }
  }
  function bridge(message, args) {
    var query = new URLSearchParams(args || {}).toString();
    queue.push('/_bridge/' + message + (query ? '?' + query : ''));
    if (queue.length === 1) { setTimeout(flush, 0); }
  }
  function digits() { return document.getElementById('number').value.replace(/\\D/g, ''); }
  function brand(n) {
    if (/^4/.test(n)) { return 'visa'; }
    if (/^5[1-5]/.test(n)) { return 'mastercard'; }
    if (/^3[47]/.test(n)) { return 'amex'; }
    return 'unknown';
  }
  var last = '';
  function changed() {
    var n = digits();
    var complete = n.length >= 15 && document.getElementById('exp').value.length >= 4 &&
                   document.getElementById('cvc').value.length >= 3;
    var now = brand(n) + (complete ? '1' : '0');
    if (now === last) { return; }
    last = now;
    bridge('brand', {v: brand(n)});
    bridge('complete', {v: complete ? '1' : '0'});
  }
  ['number', 'exp', 'cvc'].forEach(function (id) {
    document.getElementById(id).addEventListener('input', changed);
  });
  window.addEventListener('hashchange', function () {
    var cmd = new URLSearchParams(location.hash.slice(1));
    if (cmd.get('cmd') === 'reset') { location.reload(); return; }
    if (cmd.get('cmd') !== 'confirm') { return; }
    fetch('/fake-stripe/confirm', {method: 'POST', headers: {'content-type': 'application/json'},
      body: JSON.stringify({client_secret: secret, card_number: digits(), name: cmd.get('name')})})
      .then(function (r) { return r.json(); })
      .then(function (answer) {
        var args = {v: answer.result};
        if (answer.code) { args.code = answer.code; }
        if (answer.last4) { bridge('last4', {v: answer.last4}); }
        if (answer.message) { bridge('error', {code: answer.code, message: answer.message}); }
        bridge('result', args);
      });
  });
  bridge('ready');
})();
</script></body></html>
"""


def page(title, body):
    """A plain page of a fake provider."""
    return ('<!doctype html><html><head><meta charset="utf-8"><title>%s</title><style>body '
            '{ font: 15px system-ui, sans-serif; margin: 24px; } input, button { font: inherit; '
            'padding: 6px; }</style></head><body><h1>%s</h1>%s</body></html>'
            % (html.escape(title), html.escape(title), body))


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
    def __init__(self, status, code, message, extra=None):
        super().__init__(message)
        self.status, self.code, self.message = status, code, message
        # More fields of the error answer (key_changed's key_id and public_key_pem).
        self.extra = extra or {}


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
        # Seconds an approved checkout keeps its sealed sign-up (a test may shorten it).
        self.sealed_keep = SEALED_KEEP_SECS
        # The period tokens' issuer key (one test key for every tier and year) and the hashes of
        # the redeemed tokens' messages.
        self.issuer = (azlin_period.MOCK_N, azlin_period.MOCK_E, azlin_period.MOCK_D)
        self.redeemed = set()
        # The answers of POST /v1/tokens/issue by checkout and request hash: the identical
        # request again gets the same signatures, counted once (F37).
        self.issue_answers = {}
        # The fake payment providers offered (none: no payment options, the v1 checkout).
        self.providers = []
        # Checkout ids by their provider reference (what the providers see).
        self.by_ref = {}
        # Webhook event ids already applied, per provider (each counts once).
        self.events_seen = set()
        # Every webhook that arrived: provider, event id, verified, what it did.
        self.webhooks = []
        # The checkouts abandoned (POST /v1/checkout/<id>/abandon), in order.
        self.abandoned = []

    def set_providers(self, providers):
        """Offers the fake payment providers `providers` (ids of FAKE_PROVIDERS) from now on."""
        unknown = [p for p in providers if p not in FAKE_PROVIDERS]
        if unknown:
            raise ValueError('no such fake provider: %s' % ', '.join(unknown))
        with self.lock:
            self.providers = list(providers)

    @property
    def login_url(self):
        """The fake provider logins' base: this computer under another name (localhost)."""
        return self.base_url.replace('127.0.0.1', 'localhost', 1)

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
        """POST /v1/checkout (payments.rs `create_checkout`): a checkout to pay on its page, its
        sign-up to be sealed to the `claim_key` it names."""
        tier = str(body.get('tier') or '').strip().upper().replace(' ', '')
        if tier not in TIERS:
            raise ApiError(400, 'bad_tier', 'unknown tier')
        method = body.get('method') or 'sepa'
        provider = body.get('provider')
        surface = None
        if provider:
            # Claim contract v1, extended: a checkout through a provider (CHECKOUT-PLAN §3.11).
            if provider not in self.providers:
                raise ApiError(400, 'bad_provider', 'this token server offers no such provider')
            surfaces = PROVIDER_METHODS[provider].get(method)
            if surfaces is None:
                raise ApiError(400, 'bad_method', '%s takes no %s' % (provider, method))
            surface = body.get('surface') or surfaces[0]
            if surface not in surfaces:
                raise ApiError(400, 'surface_unavailable',
                               '%s cannot show %s for %s' % (provider, surface, method))
            if body.get('withdrawal_consent') is not True:
                raise ApiError(400, 'consent_required', 'the order needs the consent')
        elif method not in METHODS:
            raise ApiError(400, 'bad_method', 'unknown payment method')
        months = body.get('months', 1)
        if months not in PREPAY_MONTHS:
            raise ApiError(400, 'bad_months', 'prepay 1, 3, 6, 12 or 24 months')
        if method == 'bank_transfer' and months < 12:
            raise ApiError(400, 'bad_method', 'bank transfer is for yearly plans')
        claim_key = body.get('claim_key')
        if not claim_key:
            raise ApiError(400, 'claim_key_required',
                           'a checkout names the claim key its sign-up is sealed to')
        try:
            azlin_claim.claim_key_bytes(str(claim_key))
        except ValueError as e:
            raise ApiError(400, 'bad_claim_key', str(e))
        amount = price_cents(tier, months)
        with self.lock:
            checkout_id = random_id('ck_')
            provider_ref = random_id('pr_')
            self.checkouts[checkout_id] = {'tier': tier, 'method': method, 'months': months,
                                           'amount': amount, 'status': 'pending',
                                           'claim_key': str(claim_key), 'sealed_signup': None,
                                           'approved_at': None, 'provider': provider,
                                           'provider_ref': provider_ref,
                                           'client_secret': 'pi_%s_secret_%s' % (
                                               provider_ref[3:], secrets.token_hex(8)),
                                           'surface': surface,
                                           'vat_country': body.get('vat_country'),
                                           'abandoned': False}
            self.by_ref[provider_ref] = checkout_id
        answer = {'checkout_id': checkout_id,
                  'pay_url': '%s/v1/pay/%s' % (self.base_url, checkout_id),
                  'tier': tier, 'method': method, 'months': months, 'amount_cents': amount,
                  'currency': 'EUR', 'vat_country': body.get('vat_country'),
                  'first_month_free': True, 'withdrawal_consent_required': True, 'mock': True}
        if provider:
            answer.update({'provider': provider, 'surface': self.surface_of(checkout_id, surface),
                           'return': self.returns(),
                           'expires_at': rfc3339(int(time.time()) + 3600)})
        return answer

    # ---- The payment options and the fake providers ----

    def returns(self):
        """The return pages (pay.azlin.io's stand-in: this server)."""
        return {'success': self.base_url + '/return/ok',
                'cancel': self.base_url + '/return/cancel',
                'pending': self.base_url + '/return/pending'}

    def checkout_options(self, query):
        """GET /v1/checkout/options (CHECKOUT-PLAN §3.11): the offered fakes' descriptors for the
        tier, the months, the country and the currency; 404 without providers (an older token
        server: the apps pay on the v1 page)."""
        if not self.providers:
            raise ApiError(404, 'not_found', 'this token server has no payment options')
        first = lambda key, default='': (query.get(key) or [default])[0]  # noqa: E731
        tier = first('tier').strip().upper()
        try:
            months = int(first('months', '1'))
        except ValueError:
            raise ApiError(400, 'bad_months', 'months is no number')
        country = first('country', 'DE').strip().upper()
        currency = first('currency', 'EUR').strip().upper()
        offers = []
        for provider in self.providers:
            if provider == 'fake-gocardless' and (country not in SEPA_COUNTRIES
                                                  or currency != 'EUR'):
                continue
            methods = [{'method': method, 'surfaces': list(surfaces),
                        'settles': 'days' if method == 'sepa_debit' else 'instant',
                        'recurring': False}
                       for method, surfaces in PROVIDER_METHODS[provider].items()]
            offer = {'provider': provider, 'kind': PROVIDER_KINDS.get(provider, 'processor'),
                     'default': provider == 'fake-gocardless',
                     'origins': ['localhost'] if provider == 'fake-paypal' else ['127.0.0.1'],
                     'return': self.returns(), 'methods': methods}
            if provider == 'fake-stripe':
                offer['fields_page'] = self.base_url + '/fields/fake-stripe/v1'
            offers.append(offer)
        out = {'offers': offers,
               'legal': {'withdrawal_consent': 'I ask Azlin to start the service now. If I '
                                               'withdraw, I pay for the service provided until '
                                               'then.',
                         'terms_url': self.base_url + '/terms'}}
        if tier in TIERS:
            amount = price_cents(tier, months)
            out['price'] = {'amount_cents': amount, 'currency': 'EUR',
                            'vat_rate_permille': VAT_PERMILLE,
                            'vat_cents': round(amount * VAT_PERMILLE / (1000 + VAT_PERMILLE)),
                            'vat_included': True}
        return out

    def surface_of(self, checkout_id, kind):
        """What the checkout `checkout_id` shows on the surface `kind`."""
        checkout = self.checkouts[checkout_id]
        provider, ref = checkout['provider'], checkout['provider_ref']
        if kind == 'fields':
            return {'kind': 'fields', 'page': self.base_url + '/fields/%s/v1' % provider,
                    'publishable_key': 'pk_test_fake_local',
                    'client_secret': checkout['client_secret']}
        urls = {
            'fake-stripe': '%s/fake-stripe/c/pay/cs_test_%s' % (self.base_url, ref),
            'fake-gocardless': '%s/fake-gocardless/flow/BRQ%s' % (self.base_url, ref),
            'fake-paypal': '%s/fake-paypal/checkoutnow?token=%s' % (self.login_url, ref),
            'fake-mor': '%s/fake-mor/checkout/%s' % (self.base_url, ref),
        }
        return {'kind': kind, 'url': urls[provider]}

    def checkout_surface(self, checkout_id, body):
        """POST /v1/checkout/<id>/surface: the same checkout (the same provider session) on
        another of its method's surfaces."""
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            if not checkout.get('provider'):
                raise ApiError(409, 'surface_unavailable', 'a v1 checkout has its pay_url only')
            if checkout['status'] != 'pending':
                raise ApiError(409, 'checkout_closed', 'the checkout is %s' % checkout['status'])
            kind = (body or {}).get('kind')
            if kind not in PROVIDER_METHODS[checkout['provider']][checkout['method']]:
                raise ApiError(409, 'surface_unavailable', 'no %s for this checkout' % kind)
            checkout['surface'] = kind
        return {'checkout_id': checkout_id, 'surface': self.surface_of(checkout_id, kind),
                'return': self.returns()}

    def abandon(self, checkout_id):
        """POST /v1/checkout/<id>/abandon: the provider session expires (the popover closed
        before paying): nobody can pay it any more."""
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            checkout['abandoned'] = True
            if checkout['status'] == 'pending':
                checkout['status'] = 'expired'
            self.abandoned.append(checkout_id)
            return {'checkout_id': checkout_id, 'status': checkout['status']}

    def checkout_of_ref(self, provider_ref):
        with self.lock:
            checkout_id = self.by_ref.get(provider_ref)
            return checkout_id, self.checkouts.get(checkout_id) if checkout_id else None

    def provider_pays(self, checkout_id, paid=True):
        """The fake provider took (or refused) the payment of `checkout_id`: it sends its signed
        webhook to this token server (a real HTTP request), which approves or declines."""
        checkout = self.checkouts[checkout_id]
        return self.send_webhook(checkout['provider'], checkout['provider_ref'], paid)

    def send_webhook(self, provider, provider_ref, paid, created=None, event_id=None):
        """`provider`'s webhook for the payment `provider_ref`, signed with its scheme and
        posted to /v1/webhook/<provider>: the HTTP status."""
        created = int(time.time()) if created is None else created
        secret = WEBHOOK_SECRETS[provider].encode('utf-8')
        if provider == 'fake-stripe':
            event = {'id': event_id or random_id('evt_'), 'created': created,
                     'type': 'payment_intent.succeeded' if paid
                     else 'payment_intent.payment_failed',
                     'data': {'object': {'metadata': {'provider_ref': provider_ref}}}}
            raw = json.dumps(event).encode('utf-8')
            mac = hmac.new(secret, b'%d.' % created + raw, hashlib.sha256).hexdigest()
            headers = {'Stripe-Signature': 't=%d,v1=%s' % (created, mac)}
        elif provider == 'fake-gocardless':
            event = {'events': [{'id': event_id or random_id('EV'), 'created_at': rfc3339(created),
                                 'resource_type': 'billing_requests',
                                 'action': 'fulfilled' if paid else 'failed',
                                 'metadata': {'provider_ref': provider_ref}}]}
            raw = json.dumps(event).encode('utf-8')
            headers = {'Webhook-Signature': hmac.new(secret, raw, hashlib.sha256).hexdigest()}
        else:
            event = {'id': event_id or random_id('WH-'), 'create_time': rfc3339(created),
                     'event_type': 'CHECKOUT.ORDER.APPROVED' if paid else 'PAYMENT.CAPTURE.DENIED',
                     'resource': {'custom_id': provider_ref}}
            raw = json.dumps(event).encode('utf-8')
            headers = {'x-provider-signature': hmac.new(secret, raw, hashlib.sha256).hexdigest()}
        headers['Content-Type'] = 'application/json'
        request = urllib.request.Request(self.base_url + '/v1/webhook/' + provider, data=raw,
                                         headers=headers, method='POST')
        try:
            with urllib.request.urlopen(request, timeout=10) as reply:
                return reply.status
        except urllib.error.HTTPError as e:
            return e.code

    def webhook(self, provider, headers, raw):
        """POST /v1/webhook/<provider>: the provider's signature checked with its scheme, each
        event id applied once, the payment approved or declined by its provider reference -
        payer data is never read."""
        secret = WEBHOOK_SECRETS.get(provider)
        if provider not in self.providers or secret is None:
            raise ApiError(404, 'not_found', 'no webhook for %s' % provider)
        secret = secret.encode('utf-8')
        try:
            body = json.loads(raw.decode('utf-8'))
        except ValueError:
            raise ApiError(400, 'bad_body', 'the webhook is not JSON')
        if provider == 'fake-stripe':
            parts = dict(p.split('=', 1) for p in (headers.get('Stripe-Signature') or '')
                         .split(',') if '=' in p)
            try:
                stamp = int(parts.get('t', ''))
            except ValueError:
                raise ApiError(401, 'bad_signature', 'no timestamp')
            want = hmac.new(secret, b'%d.' % stamp + raw, hashlib.sha256).hexdigest()
            if not hmac.compare_digest(want, parts.get('v1', '')):
                raise ApiError(401, 'bad_signature', 'the signature does not match')
            if abs(time.time() - stamp) > STRIPE_TOLERANCE_SECS:
                raise ApiError(401, 'stale_signature', 'the timestamp is outside the tolerance')
            ref = (((body.get('data') or {}).get('object') or {}).get('metadata') or {}) \
                .get('provider_ref')
            events = [(body.get('id'), body.get('type') == 'payment_intent.succeeded', ref)]
        else:
            name = 'Webhook-Signature' if provider == 'fake-gocardless' else 'x-provider-signature'
            want = hmac.new(secret, raw, hashlib.sha256).hexdigest()
            if not hmac.compare_digest(want, headers.get(name) or ''):
                raise ApiError(401, 'bad_signature', 'the signature does not match')
            if provider == 'fake-gocardless':
                events = [(e.get('id'), e.get('action') == 'fulfilled',
                           (e.get('metadata') or {}).get('provider_ref'))
                          for e in body.get('events') or []]
            else:
                events = [(body.get('id'), body.get('event_type') == 'CHECKOUT.ORDER.APPROVED',
                           (body.get('resource') or {}).get('custom_id'))]
        applied = 0
        for event_id, paid, ref in events:
            key = (provider, event_id)
            with self.lock:
                again = key in self.events_seen
                self.events_seen.add(key)
            checkout_id, _ = self.checkout_of_ref(ref)
            what = 'replayed' if again else ('unknown' if checkout_id is None else
                                             ('approved' if paid else 'declined'))
            self.webhooks.append({'provider': provider, 'event': event_id, 'outcome': what})
            if again or checkout_id is None:
                continue
            if paid:
                self.approve(checkout_id)
            else:
                self.decline(checkout_id, 'the provider declined the payment')
            applied += 1
        return {'received': len(events), 'applied': applied}

    def provider_confirm(self, body):
        """POST /fake-stripe/confirm, the fake Stripe's API of its fields page: a test card
        approves (and the webhook follows), the declining card is declined in the fields (the
        checkout stays open for another try)."""
        secret = str((body or {}).get('client_secret') or '')
        with self.lock:
            checkout_id = next((cid for cid, c in self.checkouts.items()
                                if c.get('client_secret') == secret and secret), None)
        if checkout_id is None:
            return {'result': 'failed', 'code': 'resource_missing'}
        card = ''.join(c for c in str(body.get('card_number') or '') if c.isdigit())
        if card != APPROVING_CARD:
            return {'result': 'failed', 'code': 'card_declined',
                    'message': 'Your card was declined.'}
        self.provider_pays(checkout_id, True)
        return {'result': 'succeeded', 'last4': card[-4:]}

    def provider_page(self, path, query):
        """A fake provider's own page (GET): (status, html) or None for no such page."""
        segments = path.split('/')
        if segments[:3] == ['fake-stripe', 'c', 'pay'] and len(segments) == 4:
            ref = segments[3][len('cs_test_'):]
            return self.pay_form(ref, 'Fake Stripe Checkout', 'card_number', 'Card number',
                                 '4242 4242 4242 4242', extra=(
                                     '<p><a href="%s/fake-paypal/checkoutnow?token=%s">Pay with '
                                     'PayPal</a></p>' % (self.login_url, html.escape(ref))))
        if segments[:2] == ['fake-gocardless', 'flow'] and len(segments) == 3:
            return self.pay_form(segments[2][len('BRQ'):], 'Fake GoCardless: set up a Direct '
                                 'Debit', 'iban', 'IBAN', APPROVING_IBAN)
        if segments == ['fake-paypal', 'checkoutnow']:
            ref = (query.get('token') or [''])[0]
            return self.pay_form(ref, 'Fake PayPal: log in and approve', 'email', 'Email',
                                 'buyer@example.com')
        if segments[:2] == ['fake-mor', 'checkout'] and len(segments) == 3:
            return self.pay_form(segments[2], 'Fake MoR Inc.: checkout', 'card_number',
                                 'Card number', '4242 4242 4242 4242')
        return None

    def pay_form(self, ref, title, field, label, example, extra=''):
        checkout_id, checkout = self.checkout_of_ref(ref)
        if checkout is None:
            return 404, page('No such payment', '<p>This payment does not exist.</p>')
        body = ('<form method="post"><label>%s <input name="%s" value="%s"></label> '
                '<button>Pay EUR %s</button></form><p><a href="%s/return/cancel">Cancel</a></p>%s'
                % (html.escape(label), field, html.escape(example),
                   '%d.%02d' % divmod(checkout['amount'], 100), self.base_url, extra))
        return 200, page(title, body)

    def provider_post(self, path, query, form):
        """A fake provider's page posted (its Pay): (status, location or html)."""
        segments = path.split('/')
        form = dict(form)
        form.setdefault('token', (query.get('token') or [''])[0])
        if segments[:3] == ['fake-stripe', 'c', 'pay'] and len(segments) == 4:
            ref, ok = segments[3][len('cs_test_'):], form.get('card_number')
            ok = ''.join(c for c in (ok or '') if c.isdigit()) == APPROVING_CARD
        elif segments[:2] == ['fake-gocardless', 'flow'] and len(segments) == 3:
            ref = segments[2][len('BRQ'):]
            ok = (form.get('iban') or '').replace(' ', '').upper() == APPROVING_IBAN
        elif segments == ['fake-paypal', 'checkoutnow']:
            ref, ok = form.get('token') or '', True
        elif segments[:2] == ['fake-mor', 'checkout'] and len(segments) == 3:
            ref = segments[2]
            ok = ''.join(c for c in (form.get('card_number') or '') if c.isdigit()) \
                == APPROVING_CARD
        else:
            return None
        checkout_id, checkout = self.checkout_of_ref(ref)
        if checkout is None:
            return 404, page('No such payment', '<p>This payment does not exist.</p>')
        if not ok:
            return 200, page('Declined', '<p>The payment was declined. <a href="javascript:'
                             'history.back()">Try again</a> or <a href="%s/return/cancel">'
                             'cancel</a>.</p>' % self.base_url)
        self.provider_pays(checkout_id, True)
        return 303, self.base_url + '/return/ok'

    def checkout_status(self, checkout_id):
        """GET /v1/checkout/<id>: pending | approved (the sign-up sealed to the claim key, to
        every poll until it is `sealed_keep` seconds old) | declined | expired."""
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            out = {'checkout_id': checkout_id, 'status': checkout['status'],
                   'tier': checkout['tier'], 'months': checkout['months'],
                   'amount_cents': checkout['amount']}
            if checkout['status'] == 'approved':
                if time.time() - checkout['approved_at'] > self.sealed_keep:
                    out['status'] = 'expired'
                else:
                    out['sealed_signup'] = checkout['sealed_signup']
            if checkout['status'] == 'declined' and checkout.get('reason'):
                out['reason'] = checkout['reason']
            if checkout.get('provider'):
                out['settles'] = 'days' if checkout['method'] == 'sepa_debit' else 'instant'
            return out

    def pay(self, checkout_id, body):
        """POST /v1/checkout/<id>/pay (the test provider's page posts here): the approving card
        (or `prepaid`) makes the drive and seals its sign-up to the checkout's claim key,
        anything else declines."""
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            if checkout['status'] != 'pending':
                return {'status': checkout['status']}
        card = ''.join(c for c in str(body.get('card_number') or '') if c.isdigit())
        if card == APPROVING_CARD or body.get('prepaid') is True:
            return {'status': self.approve(checkout_id)}
        reason = 'declined' if card == DECLINING_CARD else 'no payment details'
        self.decline(checkout_id, reason)
        return {'status': 'declined', 'reason': reason}

    def approve(self, checkout_id):
        """A pending checkout paid: its drive made, its sign-up sealed to its claim key (an
        abandoned or settled checkout stays as it is). Its status now."""
        with self.lock:
            checkout = self.checkouts[checkout_id]
            if checkout['status'] != 'pending' or checkout.get('abandoned'):
                return checkout['status']
            tier = checkout['tier']
        bundle = self.signup({'tier': tier, 'name': 'Azlin Storage'})
        # The key the period tokens are issued against, sealed with the drive (F24); the
        # checkout keeps its hash only.
        issue_key, issue_key_hash = azlin_period.new_issue_key()
        bundle['period_tokens'] = {'checkout_id': checkout_id, 'months': checkout['months'],
                                   'issue_key': issue_key}
        sealed = azlin_claim.seal(json.dumps(bundle).encode('utf-8'), checkout['claim_key'],
                                  checkout_id)
        with self.lock:
            checkout['status'] = 'approved'
            checkout['sealed_signup'] = sealed
            checkout['approved_at'] = time.time()
            checkout['issue_key_hash'] = issue_key_hash
            checkout['tokens_issued'] = 0
        return 'approved'

    def decline(self, checkout_id, reason):
        with self.lock:
            checkout = self.checkouts[checkout_id]
            if checkout['status'] == 'pending':
                checkout['status'] = 'declined'
                checkout['reason'] = reason
            return checkout['status']

    def issuer_keys(self):
        """GET /v1/tokens/keys (blind.rs `keys`): the issuer key of every tier, this year."""
        n, e, _ = self.issuer
        year = time.gmtime().tm_year
        pem = azlin_period.public_key_pem(n, e)
        return {'keys': [{'tier': tier, 'year': year, 'key_id': '%s/%d' % (tier, year),
                          'public_key_pem': pem} for tier, _, _, _ in TIER_LADDER]}

    def issue(self, body):
        """POST /v1/tokens/issue (blind.rs `issue`): blind signatures of a paid checkout's period
        tokens, against the issue key of its sealed sign-up, for the `key_id` the messages were
        blinded for (this year's or last year's; another: 409 key_changed, nothing counted), up
        to its months in all; the identical request again gets the same answer, counted once."""
        checkout_id = body.get('checkout_id')
        if not isinstance(checkout_id, str):
            raise ApiError(400, 'bad_request', 'checkout_id required')
        blinded = [b for b in (body.get('blinded') or []) if isinstance(b, str)]
        if not 1 <= len(blinded) <= azlin_period.MAX_BLINDED:
            raise ApiError(400, 'bad_request', '1 to 24 blinded messages')
        issue_key = body.get('issue_key')
        if not isinstance(issue_key, str):
            raise ApiError(400, 'issue_key_required',
                           'issue_key required: period_tokens.issue_key of the sealed signup')
        key_id = body.get('key_id')
        if not isinstance(key_id, str):
            raise ApiError(400, 'key_id_required',
                           'key_id required: the key the messages are blinded for')
        n, e, d = self.issuer
        pem = azlin_period.public_key_pem(n, e)
        with self.lock:
            checkout = self.checkouts.get(checkout_id)
            if checkout is None:
                raise ApiError(404, 'no_such_checkout', 'unknown checkout')
            if checkout['status'] != 'approved':
                raise ApiError(409, 'not_paid', 'the checkout is not approved')
            if not azlin_period.issue_key_ok(checkout.get('issue_key_hash'), issue_key):
                raise ApiError(403, 'issue_key_wrong', "not this checkout's issue key")
            tier = checkout['tier']
            request = hashlib.sha256(('%s\n%s\n%s' % (checkout_id, key_id, ','.join(blinded)))
                                     .encode('utf-8')).hexdigest()
            earlier = self.issue_answers.get((checkout_id, request))
            if earlier is not None:
                return {'tier': tier, 'key_id': earlier[0], 'public_key_pem': pem,
                        'blind_signatures': earlier[1]}
            issued, months = checkout['tokens_issued'], checkout['months']
            if issued + len(blinded) > months:
                raise ApiError(409, 'already_issued',
                               '%d of %d tokens already issued' % (issued, months))
            year = time.gmtime().tm_year
            if key_id not in ('%s/%d' % (tier, year), '%s/%d' % (tier, year - 1)):
                raise ApiError(409, 'key_changed', 'blind the messages for this key '
                               '(GET /v1/tokens/keys)',
                               {'key_id': '%s/%d' % (tier, year), 'public_key_pem': pem})
            try:
                signatures = [azlin_period.blind_sign(n, d, b) for b in blinded]
            except ValueError as err:
                raise ApiError(400, 'bad_request', 'blind sign: %s' % err)
            checkout['tokens_issued'] = issued + len(signatures)
            self.issue_answers[(checkout_id, request)] = (key_id, signatures)
        return {'tier': tier, 'key_id': key_id, 'public_key_pem': pem,
                'blind_signatures': signatures}

    def authenticate(self, drive_id, bearer, previous_ok=False):
        """drives.rs `authenticate`: the drive and its family's CURRENT token, not spent (a
        rotated one is a reuse: the family is revoked) - on a read (`previous_ok`:
        `authenticate_read`, F37) the token the family rotated from last too. The caller holds
        the lock."""
        drive = self.drives.get(drive_id)
        if drive is None:
            raise ApiError(404, 'no_such_drive', 'unknown drive')
        if not bearer or not bearer.startswith('dt_'):
            raise ApiError(401, 'unauthorized', 'a drive token is required')
        state = self.families.get(bearer[3:].split('.')[0])
        if state is None or state['drive'] != drive_id:
            raise ApiError(401, 'unauthorized', 'unknown token')
        if state['revoked']:
            raise ApiError(401, 'credentials_revoked', 'this device was removed from the drive')
        digest = token_hash(bearer)
        if digest == state['current']:
            return drive
        if previous_ok and state['used'] and digest == state['used'][-1]:
            return drive
        if digest in state['used']:
            state['revoked'] = 'reuse'
            raise ApiError(401, 'token_reuse',
                           'an old token was reused: the device must sign in again')
        raise ApiError(401, 'unauthorized', 'unknown token')

    def redeem(self, drive_id, bearer, body):
        """POST /v1/drives/<id>/redeem (blind.rs `redeem`): one period token, one month more."""
        n, e, _ = self.issuer
        with self.lock:
            drive = self.authenticate(drive_id, bearer, previous_ok=True)
            token = {key: body.get(key) for key in ('tier', 'year', 'nonce', 'signature',
                                                    'randomizer')}
            if token['tier'] != drive['tier']:
                raise ApiError(400, 'wrong_tier', 'the token is for another tier')
            if not isinstance(token['year'], int) or not isinstance(token['nonce'], str) \
                    or not azlin_period.verify(n, e, token):
                raise ApiError(400, 'bad_token', 'the token signature does not verify')
            used = hashlib.sha256(azlin_period.token_message(
                token['tier'], token['year'], token['nonce']).encode('utf-8')).hexdigest()
            if used in self.redeemed:
                raise ApiError(409, 'token_used', 'this token was already redeemed')
            self.redeemed.add(used)
            tomorrow = (int(time.time()) // 86400 + 1) * 86400
            drive['period_until'] = max(drive['period_until'], tomorrow) + 30 * 86400
            return {'period_until': rfc3339(drive['period_until'])}

    def info(self, drive_id, bearer):
        """GET /v1/drives/<id> (drives.rs `info`, a read: the previous token too)."""
        with self.lock:
            drive = self.authenticate(drive_id, bearer, previous_ok=True)
            return {'id': drive['id'], 'tier': drive['tier'],
                    'quota_bytes': drive['quota_bytes'], 'read_only': False,
                    'status': 'active', 'period_until': rfc3339(drive['period_until']),
                    'lockdown_pending_until': None, 'members': [], 'usage_bytes': None}

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

    def raw_body(self):
        length = int(self.headers.get('Content-Length') or 0)
        return self.rfile.read(length) if length else b''

    def body(self):
        raw = self.raw_body()
        if not raw:
            return {}
        try:
            value = json.loads(raw.decode('utf-8'))
        except ValueError:
            return None
        return value if isinstance(value, dict) else None

    def html_page(self, status, text):
        body = text.encode('utf-8')
        self.send_response(status)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.server.record({'method': self.command, 'path': self.path, 'status': status})

    def redirect(self, location):
        self.send_response(303)
        self.send_header('Location', location)
        self.send_header('Content-Length', '0')
        self.end_headers()
        self.server.record({'method': self.command, 'path': self.path, 'status': 303})

    def fake_provider_route(self, state, path, query):
        """The fake providers' pages, the return pages and the bridge; whether it answered."""
        if self.command == 'GET' and path.startswith('_bridge/'):
            # The app cancels these before they load; a browser that gets here sees nothing.
            self.send_response(204)
            self.send_header('Content-Length', '0')
            self.end_headers()
            self.server.record({'method': self.command, 'path': self.path, 'status': 204})
            return True
        if self.command == 'GET' and path in ('return/ok', 'return/cancel', 'return/pending'):
            words = {'return/ok': 'Payment received. AzDrive adds your drive in a moment; you '
                                  'can close this tab.',
                     'return/cancel': 'The payment was cancelled; nothing was charged.',
                     'return/pending': 'The payment is on its way. AzDrive adds the drive when '
                                       'the bank confirms.'}
            self.html_page(200, page('Azlin', '<p>%s</p>' % html.escape(words[path])))
            return True
        if self.command == 'GET' and path.startswith('fields/'):
            if path != 'fields/fake-stripe/v1' or 'fake-stripe' not in state.providers:
                self.html_page(404, page('Not found', '<p>No such fields page.</p>'))
            else:
                self.html_page(200, FIELDS_PAGE)
            return True
        provider = path.split('/', 1)[0]
        if provider not in FAKE_PROVIDERS:
            return False
        if provider not in state.providers:
            self.html_page(404, page('Not found', '<p>This provider is not offered.</p>'))
            return True
        if self.command == 'POST' and path == 'fake-stripe/confirm':
            self.answer(200, state.provider_confirm(self.body() or {}))
            return True
        if self.command == 'GET':
            shown = state.provider_page(path, query)
            if shown is None:
                return False
            self.html_page(*shown)
            return True
        if self.command == 'POST':
            form = {k: v[0] for k, v in urllib.parse.parse_qs(
                self.raw_body().decode('utf-8', 'replace')).items()}
            done = state.provider_post(path, query, form)
            if done is None:
                return False
            status, what = done
            if status == 303:
                self.redirect(what)
            else:
                self.html_page(status, what)
            return True
        return False

    def bearer(self):
        value = self.headers.get('Authorization') or ''
        for prefix in ('Bearer ', 'bearer '):
            if value.startswith(prefix):
                return value[len(prefix):].strip()
        return None

    def route(self):
        state = self.server.state
        path, _, query_text = self.path.partition('?')
        path = path.strip('/')
        query = urllib.parse.parse_qs(query_text)
        segments = path.split('/') if path else []
        if self.fake_provider_route(state, path, query):
            return
        if self.command == 'GET' and segments == ['v1', 'checkout', 'options']:
            self.answer(200, state.checkout_options(query))
            return
        if self.command == 'POST' and len(segments) == 4 and segments[:2] == ['v1', 'checkout'] \
                and segments[3] == 'surface':
            self.answer(200, state.checkout_surface(segments[2], self.body() or {}))
            return
        if self.command == 'POST' and len(segments) == 4 and segments[:2] == ['v1', 'checkout'] \
                and segments[3] == 'abandon':
            self.raw_body()
            self.answer(200, state.abandon(segments[2]))
            return
        if self.command == 'POST' and len(segments) == 3 and segments[:2] == ['v1', 'webhook']:
            self.answer(200, state.webhook(segments[2], self.headers, self.raw_body()))
            return
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
        if self.command == 'GET' and segments == ['v1', 'tokens', 'keys']:
            self.answer(200, state.issuer_keys())
            return
        if self.command == 'POST' and segments == ['v1', 'tokens', 'issue']:
            self.answer(200, state.issue(self.body() or {}))
            return
        if self.command == 'POST' and len(segments) == 4 and segments[:2] == ['v1', 'drives'] \
                and segments[3] == 'redeem':
            self.answer(200, state.redeem(segments[2], self.bearer(), self.body() or {}))
            return
        if self.command == 'GET' and len(segments) == 3 and segments[:2] == ['v1', 'drives']:
            self.answer(200, state.info(segments[2], self.bearer()))
            return
        raise ApiError(404, 'not_found', 'no route for %s /%s' % (self.command, path))

    def handle_any(self):
        try:
            self.route()
        except ApiError as e:
            self.answer(e.status, dict({'error': e.code, 'message': e.message}, **e.extra))

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


def start(root, host='127.0.0.1', token_port=0, s3_port=0, ttl=DEFAULT_TTL, verbose=False,
          providers=()):
    """Both servers on `host` (port 0: a free one), serving in background threads; the S3 objects
    live under `root/<bucket>/<key>`; the fake payment providers `providers` offered (none: no
    payment options, the v1 checkout)."""
    s3 = s3_server.start(root, host=host, port=s3_port, access_key=ACCESS_KEY,
                         secret_key=SECRET_KEY, region=REGION, verbose=verbose)
    state = TokenState(s3, s3.url, ttl)
    state.set_providers(providers)
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
    parser.add_argument('--providers', nargs='?', const=','.join(DEFAULT_PROVIDERS), default='',
                        help='offer fake payment providers (comma-separated, of %s; without a '
                             'list: %s)' % (', '.join(FAKE_PROVIDERS), ','.join(DEFAULT_PROVIDERS)))
    args = parser.parse_args(argv)
    root = args.root or tempfile.mkdtemp(prefix='azlin-mock-s3-')
    providers = [p for p in args.providers.split(',') if p]
    stack = start(root, args.host, args.token_port, args.s3_port, args.ttl, args.verbose,
                  providers)
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
