#!/usr/bin/env python3
"""The token server's behaviour the apps rely on, as HTTP checks: run against the Python mock and
against the real azlin-token, so the two answer the same (one suite, no silent drift).

    python3 scripts/azlin_token_conformance.py --mock
        starts scripts/azlin_mock_stack.py on free ports and checks it
    python3 scripts/azlin_token_conformance.py [--token-url URL] [--s3-url URL]
        checks a running token server: --token-url, else $AZLIN_TOKEN_URL, else the shared Azlin
        config's endpoints, else http://127.0.0.1:8081 (`azctl dev up --processes` in azul-apps
        iso/). --s3-url reaches the bucket through another address than the bundle's endpoint.

The checks (each prints `ok:` or `FAILED:`; the exit status is the number of failures):

 1. GET /health answers 200.
 2. POST /v1/drives answers 201 with a drive bundle: drive.id `d_...`, drive.location (kind s3, an
    endpoint, a region, a bucket, path style), credentials (access key, secret key, expires_at),
    drive_token `dt_<family>.<generation>.<random>`.
 3. The bundle's credentials list its bucket and put, get and delete an object (SigV4, path
    style, the session token signed).
 4. POST /v1/drives/<id>/credentials without a token: 401 `unauthorized`.
 5. With the drive token: 200, the same drive and bucket, a new drive token of the next
    generation and the same family.
 6. With the old token again: 401 `token_reuse` - and then the new one too: 401
    `credentials_revoked` (reuse revokes the family).
 7. POST /v1/drives/<a drive nobody has>/credentials: 404 `no_such_drive`.
 8. A route nobody serves: 404 `not_found`.
 9. The claim (CLAIM CONTRACT v1, scripts/azlin_claim.py): POST /v1/checkout without a
    `claim_key` is 400 `claim_key_required`; with one it is 201 with a checkout id and a payment
    page; paid with the test provider's approving card, GET /v1/checkout/<id> answers `approved`
    with a `sealed_signup` (and no plaintext `signup`) that opens with the claim secret for that
    checkout id to a drive bundle - and opens for no other checkout id; a second GET answers it
    again (kept, not deleted on read); an unknown checkout is 404.
10. The period tokens (AZLINSEC17 F24, F36, scripts/azlin_period.py): the sealed sign-up carries
    `period_tokens` with the checkout's id, its months and an issue key; GET /v1/tokens/keys names
    the tier's issuer key; POST /v1/tokens/issue without the issue key is 400
    `issue_key_required`, with another one 403 `issue_key_wrong`, without the `key_id` the
    messages were blinded for 400 `key_id_required`, with a key the server no longer signs with
    409 `key_changed` naming the current one (nothing counted), with the key and the key id 200
    and one blind signature per month that finalizes into a token the issuer key verifies; the
    identical request again answers the same signatures (counted once); one more is 409
    `already_issued`; POST /v1/drives/<id>/redeem with the drive token takes a token for a month
    more, and the same token again is 409 `token_used`.
11. The token just replaced still reads (F37): after a refresh, the previous drive token redeems
    a period token and reads the drive (GET /v1/drives/<id>) without revoking the family - the
    new token refreshes after that; a token older than the previous one is a reuse on a read
    too (401 `token_reuse`).
12. The recovery key (§18.7, AZLINSEC17 F14, scripts/azlin_ed25519.py): a new drive's owner
    registers an Ed25519 key (POST /v1/drives/<id>/recovery); a lockdown signed with it and no
    drive token is 202 with `pending_until` and a new family's drive token; the same request
    again is 409 `nonce_used`, one signed by another key 401; the drive's status names the
    pending lockdown; the pending family gets no credentials before the 48 hours are over (403
    `lockdown_pending`: D42, the drive is handed over only when the notice ends - so the
    recovery wrap, and with the code the drive key, stays out of reach meanwhile); the pending
    family cannot cancel it (403), the owner can (200), and then there is none to cancel (409
    `no_pending_lockdown`).
13. Vouchers (AZLINSEC17 F29), with a development server's test codes (`AZLIN-TEST-1M`: a month,
    `AZLIN-TEST-EUR10`: EUR 10, any case, never used up): one without a drive is 201 with a new
    drive's sign-up; one on a drive (its drive token) is 200 with `days_added` (its value pro
    rata, more than a month on 100GB) and a later `period_until`; an unknown code is 400
    `voucher_invalid`. A server that takes no test code (a production one) skips the section;
    `--skip-vouchers` skips it anyway; `--mock` never skips it.
14. A lockdown by a drive token (§18.7): with a member family (POST /v1/drives/<id>/members, 201)
    and an access key (POST /v1/drives/<id>/keys, 201, `AZK...`) made first - the drive's
    members (GET /v1/drives/<id>) name the new member, `you` the caller -, the owner's lockdown
    is 200 with a new drive token for the caller; the member's token and the caller's old one
    are refused (401) and the new one refreshes.
15. A restore as of a time (D38, D42): objects put, then rewritten, deleted and added after the
    time; POST /v1/drives/<id>/restore {"prefix", "as_of": RFC 3339} with the drive token is 202
    with a `request_id` and `queued`; GET /v1/drives/<id>/restore/<request> reaches `done` with
    the objects it changed, and the prefix is as it was (the one added since gone, outside it
    nothing changed); without `as_of` it is 400 `bad_request`, an unknown request 404.
16. Cash by post (cash contract v1, scripts/azlin_cash.py): POST /v1/checkout {"method": "cash"}
    without a claim key is 400 `claim_key_required`; with one it is 201 `awaiting_cash` with its
    amount, its currency, the address to post the cash to and an end 60 days on; its activation
    code is `AZC1-` and upper-case base32 in blocks of four without padding, holding the checkout
    id, the amount (u32 BE) and the currency, then ten bytes of HMAC-SHA256 by the server's cash
    key (checked with the mock's key or `--cash-key`); its poll answers `awaiting_cash` with no
    sealed sign-up; the claim code (AZK1) of the checkout id and the claim secret reads back.
    With the operator's switches (`--mock`: the mock's stand-ins for AzCtl): activated it is
    `approved` and the claim code alone opens its sealed sign-up to the drive; rejected it is
    `rejected` with the reason; one nobody activated is `expired` after 60 days.
17. A ban with a grace period (ban contract v1, `--mock`: the operator's switch): before the ban a
    public link (a presigned GET) of the drive reads; banned, its status is `banned` with
    `ban_reason` and `ban_until` and read-only, its credentials are handed out with the same
    fields, a write and a delete are refused 403 with `x-azlin-error: drive_banned`, reads and
    listings go on, its public links are refused at once, a grant (a member family) is 403
    `drive_banned`; past the end (the mock's clock advanced) its credentials and its status are
    refused 403 `drive_banned` (with the reason and the end) and its bucket refuses reads too.

Every drive token, claim secret and issue key is secret: none is printed.
"""
import argparse
import calendar
import json
import os
import re
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import azlin_cash  # noqa: E402
import azlin_claim  # noqa: E402
import azlin_client  # noqa: E402
import azlin_ed25519  # noqa: E402
import azlin_period  # noqa: E402

# The test provider's card that approves (the mock's and azlin-token's payments.rs).
APPROVING_CARD = '4242 4242 4242 4242'

TOKEN = re.compile(r'^dt_(f_[a-z2-7]+)\.(\d+)\.[A-Za-z0-9_-]+$')


class Suite:
    def __init__(self):
        self.failures = 0

    def check(self, what, ok, detail=''):
        if ok:
            print('ok: %s' % what, flush=True)
        else:
            self.failures += 1
            print('FAILED: %s %s' % (what, detail), flush=True)
        return ok


def error_code(value):
    return (value or {}).get('error') if isinstance(value, dict) else None


def unix_of(text):
    """RFC 3339 (`2026-11-07T09:15:00Z`, milliseconds or not) to seconds since 1970; None."""
    try:
        return calendar.timegm(time.strptime(str(text).split('.')[0].rstrip('Z'),
                                             '%Y-%m-%dT%H:%M:%S'))
    except ValueError:
        return None


def rfc3339(unix):
    """Seconds since 1970 as RFC 3339 (`2026-11-07T09:15:00Z`)."""
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(unix))


# A development token server's test vouchers (azlin-proto's voucher module).
TEST_ONE_MONTH = 'AZLIN-TEST-1M'
TEST_EUR10 = 'AZLIN-TEST-EUR10'


class MockOperator:
    """The operator's switches of the mock token server (its AzCtl's stand-ins): a cash
    checkout activated or rejected, a drive banned, the server's clock moved on."""

    def __init__(self, state):
        self.state = state

    def activate_cash(self, checkout_id):
        return self.state.activate_cash(checkout_id)

    def reject_cash(self, checkout_id, reason):
        return self.state.reject_cash(checkout_id, reason)

    def ban(self, drive_id, reason, grace_secs):
        return self.state.ban(drive_id, reason, grace_secs)

    def advance(self, secs):
        self.state.advance(secs)


def run(token_url, s3_url=None, vouchers='auto', operator=None, cash_key=None):
    """`vouchers`: 'auto' (section 13 unless the server takes no test code), 'required' (the
    mock: never skipped), 'skip'. `operator`: the operator's switches (sections 16 and 17 need
    them; the mock's: MockOperator); `cash_key`: the server's cash key, for the MAC check."""
    suite = Suite()
    client = azlin_client.TokenClient(token_url)
    print('token server %s' % token_url, flush=True)
    suite.check('GET /health answers 200', client.health() == 200)

    status, bundle, text = client.signup('azlin-conformance')
    if not suite.check('POST /v1/drives answers 201 with a bundle',
                       status == 201 and isinstance(bundle, dict), '(HTTP %d %s)' % (status, text[:200])):
        return suite.failures + 1
    drive_id, bucket, endpoint, region = azlin_client.bundle_drive(bundle)
    location = (bundle.get('drive') or {}).get('location') or {}
    credentials = bundle.get('credentials') or {}
    token = bundle.get('drive_token') or ''
    suite.check('the drive id is d_<base32>', re.match(r'^d_[a-z2-7]+$', drive_id) is not None,
                '(%r)' % drive_id)
    suite.check('the location is an S3 bucket, path style',
                location.get('kind') == 's3' and bool(bucket) and bool(endpoint) and bool(region)
                and location.get('path_style') is True, '(%r)' % location)
    # Temporary credentials expire; a token server handing out long-lived keys says so
    # (`location.auth.type` "keyring") and gives no expiry.
    long_lived = ((location.get('auth') or {}).get('type') == 'keyring')
    suite.check('the credentials have keys and an expiry (unless long-lived)',
                bool(credentials.get('access_key_id')) and bool(credentials.get('secret_access_key'))
                and ('expires_at' in credentials or long_lived),
                '(keys: %s)' % sorted(credentials))
    first = TOKEN.match(token)
    suite.check('the drive token is dt_<family>.<generation>.<random>', first is not None)

    bucket_client = azlin_client.Bucket(bundle, endpoint=s3_url)
    try:
        key = 'mail/.conformance/probe.txt'
        bucket_client.put(key, b'probe', content_type='text/plain')
        listed = bucket_client.keys('mail/.conformance/')
        got = bucket_client.get(key)
        bucket_client.delete(key)
        suite.check("the bundle's credentials list, put, get and delete in its bucket",
                    listed == [key] and got == b'probe', '(listed %r)' % listed)
    except (OSError, RuntimeError) as e:
        suite.check("the bundle's credentials list, put, get and delete in its bucket", False,
                    '(%s)' % e)

    status, value, _ = client.refresh(drive_id, None)
    suite.check('a refresh without a token is 401 unauthorized',
                status == 401 and error_code(value) == 'unauthorized', '(HTTP %d %r)' % (status, value))

    status, renewed, _ = client.refresh(drive_id, token)
    new_token = (renewed or {}).get('drive_token') or ''
    second = TOKEN.match(new_token)
    suite.check('a refresh with the drive token is 200 for the same drive and bucket',
                status == 200 and azlin_client.bundle_drive(renewed or {})[:2] == (drive_id, bucket),
                '(HTTP %d)' % status)
    suite.check('the refresh hands out the next token of the same family',
                bool(first and second) and new_token != token
                and second.group(1) == first.group(1)
                and int(second.group(2)) == int(first.group(2)) + 1)

    status, value, _ = client.refresh(drive_id, token)
    suite.check('the old token again is 401 token_reuse',
                status == 401 and error_code(value) == 'token_reuse', '(HTTP %d %r)' % (status, value))
    status, value, _ = client.refresh(drive_id, new_token)
    suite.check('after a reuse the new token is 401 credentials_revoked',
                status == 401 and error_code(value) == 'credentials_revoked',
                '(HTTP %d %r)' % (status, value))

    status, value, _ = client.refresh('d_' + 'a' * 26, token)
    suite.check('a drive nobody has is 404 no_such_drive',
                status == 404 and error_code(value) == 'no_such_drive', '(HTTP %d %r)' % (status, value))
    status, value, _ = client.call('GET', '/v1/nothing-here')
    suite.check('a route nobody serves is 404 not_found',
                status == 404 and error_code(value) == 'not_found', '(HTTP %d %r)' % (status, value))
    claim_checks(suite, client)
    recovery_checks(suite, client)
    lockdown_checks(suite, client)
    restore_checks(suite, client, s3_url)
    if vouchers == 'skip':
        print('skipped: vouchers (--skip-vouchers)', flush=True)
    else:
        voucher_checks(suite, client, required=vouchers == 'required')
    cash_checks(suite, client, operator, cash_key)
    ban_checks(suite, client, operator, s3_url)
    return suite.failures


def cash_checks(suite, client, operator, cash_key):
    """16. Cash by post: the checkout, its activation code, its poll, the claim code - and with
    the operator's switches its activation, a rejection, the end of one nobody paid."""
    order = {'tier': '100GB', 'months': 12, 'method': 'cash'}
    status, value, _ = client.call('POST', '/v1/checkout', order)
    suite.check('a cash checkout without a claim key is 400 claim_key_required',
                status == 400 and error_code(value) == 'claim_key_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    secret, claim_key = azlin_claim.new_claim_key()
    started = time.time()
    status, value, text = client.call('POST', '/v1/checkout', dict(order, claim_key=claim_key))
    value = value or {}
    checkout_id = value.get('checkout_id') or ''
    amount = value.get('amount_cents')
    currency = str(value.get('currency') or '')
    if not suite.check('a cash checkout is 201 awaiting the cash, with its amount and currency',
                       status == 201 and bool(checkout_id)
                       and value.get('status') == 'awaiting_cash'
                       and isinstance(amount, int) and amount > 0
                       and re.match(r'^[A-Z]{3}$', currency) is not None,
                       '(HTTP %d %s)' % (status, text[:200])):
        return
    code = str(value.get('activation_code') or '')
    suite.check('its activation code is AZC1- and upper-case base32 in blocks of four',
                azlin_cash.SHAPE.match(code) is not None, '(%r)' % code[:24])
    try:
        parsed = azlin_cash.parse_activation_code(code)
    except ValueError as e:
        parsed = None
        suite.check('its activation code reads back', False, '(%s)' % e)
    if parsed is not None:
        suite.check('the activation code holds the checkout id, the amount and the currency',
                    (parsed['checkout_id'], parsed['amount_cents'], parsed['currency'])
                    == (checkout_id, amount, currency),
                    '(%r %r %r)' % (parsed['checkout_id'], parsed['amount_cents'],
                                    parsed['currency']))
        if cash_key:
            suite.check("its MAC is ten bytes of HMAC-SHA256 by the server's cash key",
                        len(parsed['mac']) == azlin_cash.MAC_LEN
                        and azlin_cash.verify(parsed, cash_key))
        else:
            print('skipped: the activation code\'s MAC (no --cash-key)', flush=True)
    mail_to = value.get('mail_to') or {}
    lines = mail_to.get('lines')
    suite.check('it names the address to post the cash to',
                bool(str(mail_to.get('name') or '').strip()) and isinstance(lines, list)
                and len(lines) >= 1 and all(isinstance(l, str) and l.strip() for l in lines),
                '(%r)' % sorted(mail_to))
    expires = unix_of(value.get('expires_at'))
    suite.check('it ends 60 days after it was made',
                expires is not None and abs(expires - (started + 60 * 86400)) < 86400,
                '(%r)' % value.get('expires_at'))
    path = '/v1/checkout/' + checkout_id
    status, polled, _ = client.call('GET', path)
    polled = polled or {}
    suite.check('its poll answers awaiting_cash without a sealed sign-up',
                status == 200 and polled.get('status') == 'awaiting_cash'
                and 'sealed_signup' not in polled,
                '(HTTP %d %r)' % (status, polled.get('status')))
    claim_code = azlin_claim.claim_code(checkout_id, secret)
    suite.check('the claim code (AZK1) reads back to the checkout id and the claim secret',
                azlin_claim.parse_claim_code(claim_code) == (checkout_id, secret))
    if operator is None:
        print("skipped: a cash checkout activated, rejected and ended (the operator's switches: "
              "--mock)", flush=True)
        return
    operator.activate_cash(checkout_id)
    status, polled, _ = client.call('GET', path)
    polled = polled or {}
    sealed = polled.get('sealed_signup') or ''
    suite.check('activated by the operator it is approved with a sealed sign-up',
                status == 200 and polled.get('status') == 'approved' and bool(sealed),
                '(HTTP %d %r)' % (status, polled.get('status')))
    picked_id, picked_secret = azlin_claim.parse_claim_code(claim_code)
    try:
        bundle = json.loads(azlin_claim.open_sealed(sealed, picked_secret, picked_id))
        drive_id = (bundle.get('drive') or {}).get('id') or ''
        suite.check('the claim code alone opens its sealed sign-up to the drive',
                    drive_id.startswith('d_'), '(drive %r)' % drive_id)
    except ValueError as e:
        suite.check('the claim code alone opens its sealed sign-up to the drive', False,
                    '(%s)' % e)
    reason = 'the envelope held less than the amount'
    _, other_key = azlin_claim.new_claim_key()
    _, second, _ = client.call('POST', '/v1/checkout', dict(order, claim_key=other_key))
    second_id = (second or {}).get('checkout_id') or ''
    operator.reject_cash(second_id, reason)
    status, polled, _ = client.call('GET', '/v1/checkout/' + second_id)
    polled = polled or {}
    suite.check('rejected by the operator it is rejected with the reason',
                status == 200 and polled.get('status') == 'rejected'
                and polled.get('reason') == reason,
                '(HTTP %d %r %r)' % (status, polled.get('status'), polled.get('reason')))
    _, third_key = azlin_claim.new_claim_key()
    _, third, _ = client.call('POST', '/v1/checkout', dict(order, claim_key=third_key))
    third_id = (third or {}).get('checkout_id') or ''
    operator.advance(61 * 86400)
    status, polled, _ = client.call('GET', '/v1/checkout/' + third_id)
    suite.check('a cash checkout nobody activated is expired after 60 days',
                status == 200 and (polled or {}).get('status') == 'expired',
                '(HTTP %d %r)' % (status, (polled or {}).get('status')))


def ban_checks(suite, client, operator, s3_url):
    """17. A ban with a grace period: the status and the credentials say it, writes and links
    are refused at once, reads go on until the end; then everything is refused."""
    if operator is None:
        print("skipped: a ban (the operator's switch: --mock)", flush=True)
        return
    status, bundle, text = client.signup('azlin-conformance-ban')
    if not suite.check('a drive to ban', status == 201 and isinstance(bundle, dict),
                       '(HTTP %d %s)' % (status, text[:120])):
        return
    drive_id = azlin_client.bundle_drive(bundle)[0]
    token = bundle.get('drive_token') or ''
    bucket = azlin_client.Bucket(bundle, endpoint=s3_url)
    bucket.put('ban/kept.txt', b'kept', content_type='text/plain')
    link = bucket.presigned_get('ban/kept.txt', 600)
    status, _, body = bucket.fetch(link)
    suite.check('a public link of the drive reads before the ban',
                status == 200 and body == b'kept', '(HTTP %d)' % status)
    reason = 'conformance: spam distribution'
    operator.ban(drive_id, reason, 48 * 3600)
    path = '/v1/drives/%s' % drive_id
    status, info, _ = client.call('GET', path, bearer=token)
    info = info or {}
    until = unix_of(info.get('ban_until'))
    suite.check("the banned drive's status says banned, why and until when",
                status == 200 and info.get('status') == 'banned'
                and info.get('ban_reason') == reason and until is not None,
                '(HTTP %d %r)' % (status, {k: info.get(k) for k in ('status', 'ban_until')}))
    suite.check('a banned drive is read-only', info.get('read_only') is True,
                '(%r)' % info.get('read_only'))
    status, renewed, _ = client.refresh(drive_id, token)
    renewed = renewed or {}
    suite.check('its credentials are handed out until the end, with the ban in them',
                status == 200 and renewed.get('status') == 'banned'
                and renewed.get('ban_reason') == reason
                and unix_of(renewed.get('ban_until')) == until,
                '(HTTP %d %r)' % (status, error_code(renewed) or renewed.get('status')))
    token = renewed.get('drive_token') or token
    bucket = azlin_client.Bucket(renewed if renewed.get('credentials') else bundle,
                                 endpoint=s3_url)
    st, headers, _ = bucket.request('PUT', 'ban/new.txt', body=b'new',
                                    headers={'Content-Type': 'text/plain'})
    suite.check('a write is refused 403 drive_banned',
                st == 403 and headers.get('x-azlin-error') == 'drive_banned',
                '(HTTP %d %r)' % (st, headers.get('x-azlin-error')))
    st, headers, _ = bucket.request('DELETE', 'ban/kept.txt')
    suite.check('a delete is refused 403 drive_banned',
                st == 403 and headers.get('x-azlin-error') == 'drive_banned',
                '(HTTP %d %r)' % (st, headers.get('x-azlin-error')))
    try:
        listed = bucket.keys('ban/')
        got = bucket.get('ban/kept.txt')
        suite.check('reads and listings go on until the end',
                    listed == ['ban/kept.txt'] and got == b'kept', '(listed %r)' % listed)
    except (OSError, RuntimeError) as e:
        suite.check('reads and listings go on until the end', False, '(%s)' % e)
    status, _, _ = bucket.fetch(link)
    suite.check('its public links stop working at once', status == 403, '(HTTP %d)' % status)
    status, value, _ = client.call('POST', path + '/members', {'member': 'conformance-ban'},
                                   bearer=token)
    suite.check('a grant (a member family) is refused 403 drive_banned',
                status == 403 and error_code(value) == 'drive_banned',
                '(HTTP %d %r)' % (status, error_code(value)))
    operator.advance(48 * 3600 + 60)
    status, value, _ = client.refresh(drive_id, token)
    value = value or {}
    suite.check('past the end its credentials are refused 403 drive_banned, with why and when',
                status == 403 and error_code(value) == 'drive_banned'
                and value.get('ban_reason') == reason
                and unix_of(value.get('ban_until')) == until,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=token)
    suite.check('past the end its status is refused 403 drive_banned',
                status == 403 and error_code(value) == 'drive_banned',
                '(HTTP %d %r)' % (status, error_code(value)))
    st, headers, _ = bucket.request('GET', 'ban/kept.txt')
    suite.check('past the end its bucket refuses reads too',
                st == 403 and headers.get('x-azlin-error') == 'drive_banned',
                '(HTTP %d %r)' % (st, headers.get('x-azlin-error')))


def lockdown_checks(suite, client):
    """14. A lockdown by a drive token: every family at once, the caller in a new one."""
    status, bundle, text = client.signup('azlin-conformance-lockdown')
    if not suite.check('a drive to lock down', status == 201 and isinstance(bundle, dict),
                       '(HTTP %d %s)' % (status, text[:120])):
        return
    drive_id, _, _, _ = azlin_client.bundle_drive(bundle)
    owner = bundle.get('drive_token') or ''
    path = '/v1/drives/%s' % drive_id
    status, member, _ = client.call('POST', path + '/members', {'member': 'conformance'},
                                    bearer=owner)
    member_token = (member or {}).get('drive_token') or ''
    suite.check('the owner adds a member family (201)', status == 201 and bool(member_token),
                '(HTTP %d %r)' % (status, error_code(member)))
    status, info, _ = client.call('GET', path, bearer=member_token)
    names = [m.get('member') for m in (info or {}).get('members') or [] if isinstance(m, dict)]
    suite.check("the drive's members name the new one, and `you` the caller",
                status == 200 and 'conformance' in names and len(names) >= 2
                and (info or {}).get('you') == 'conformance',
                '(HTTP %d members %r you %r)' % (status, names, (info or {}).get('you')))
    status, key, _ = client.call('POST', path + '/keys', {'perms': 'r', 'expires_days': 1},
                                 bearer=owner)
    suite.check('the owner makes an access key (201, AZK...)',
                status == 201 and str((key or {}).get('access_key_id') or '').startswith('AZK'),
                '(HTTP %d %r)' % (status, error_code(key)))
    status, locked, _ = client.call('POST', path + '/lockdown', {}, bearer=owner)
    new_token = (locked or {}).get('drive_token') or ''
    suite.check("the owner's lockdown is 200 with a new drive token",
                status == 200 and bool(new_token) and new_token not in (owner, member_token),
                '(HTTP %d %r)' % (status, error_code(locked)))
    status, value, _ = client.refresh(drive_id, member_token)
    suite.check("the member's token is refused after the lockdown (401)", status == 401,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.refresh(drive_id, owner)
    suite.check("the caller's old token is refused too (401)", status == 401,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.refresh(drive_id, new_token)
    suite.check('the new token refreshes', status == 200,
                '(HTTP %d %r)' % (status, error_code(value)))


def restore_checks(suite, client, s3_url):
    """15. A restore of a prefix as of a time."""
    status, bundle, text = client.signup('azlin-conformance-restore')
    if not suite.check('a drive to restore', status == 201 and isinstance(bundle, dict),
                       '(HTTP %d %s)' % (status, text[:120])):
        return
    drive_id, _, _, _ = azlin_client.bundle_drive(bundle)
    owner = bundle.get('drive_token') or ''
    path = '/v1/drives/%s/restore' % drive_id
    bucket = azlin_client.Bucket(bundle, endpoint=s3_url)
    try:
        bucket.put('restore/a.txt', b'a1')
        bucket.put('restore/b.txt', b'b1')
        bucket.put('kept.txt', b'k1')
        # The object times are whole seconds at the node: the time sits between two of them.
        time.sleep(1.2)
        as_of = int(time.time())
        time.sleep(1.2)
        bucket.put('restore/a.txt', b'encrypted')
        bucket.delete('restore/b.txt')
        bucket.put('restore/note.txt', b'pay')
        bucket.put('kept.txt', b'k2')
    except (OSError, RuntimeError) as e:
        suite.check('the objects to restore are put', False, '(%s)' % e)
        return
    status, value, _ = client.call('POST', path, {'prefix': 'restore/'}, bearer=owner)
    suite.check('a restore without as_of is 400 bad_request',
                status == 400 and error_code(value) == 'bad_request',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path, {'prefix': 'restore/', 'as_of': rfc3339(as_of)},
                                   bearer=owner)
    request = (value or {}).get('request_id') or ''
    if not suite.check('a restore as of a time is 202 queued with a request id',
                       status == 202 and bool(request) and (value or {}).get('status') == 'queued',
                       '(HTTP %d %r)' % (status, value)):
        return
    deadline = time.time() + 60
    state = {}
    while time.time() < deadline:
        status, state, _ = client.call('GET', '%s/%s' % (path, request), bearer=owner)
        if status != 200 or (state or {}).get('status') not in ('queued', 'running'):
            break
        time.sleep(1)
    suite.check('the restore is done, with the objects it changed',
                status == 200 and (state or {}).get('status') == 'done'
                and ((state or {}).get('objects') or 0) >= 3, '(HTTP %d %r)' % (status, state))
    try:
        back = (bucket.get('restore/a.txt'), bucket.get('restore/b.txt'),
                bucket.keys('restore/'), bucket.get('kept.txt'))
    except (OSError, RuntimeError) as e:
        back = ('(%s)' % e,)
    suite.check('the prefix is as it was, the object added since gone, outside it nothing changed',
                back == (b'a1', b'b1', ['restore/a.txt', 'restore/b.txt'], b'k2'), '(%r)' % (back,))
    status, value, _ = client.call('GET', '%s/r_nosuchrequest' % path, bearer=owner)
    suite.check('an unknown restore request is 404', status == 404, '(HTTP %d)' % status)


def voucher_checks(suite, client, required):
    """13. Vouchers with the development server's test codes."""
    status, bundle, text = client.call('POST', '/v1/vouchers/redeem',
                                       {'code': TEST_ONE_MONTH.lower(), 'tier': '100GB'})
    if not required and status == 400 and error_code(bundle) == 'voucher_invalid':
        print('skipped: vouchers (the server takes no test voucher - a production one)',
              flush=True)
        return
    drive_id = ((bundle or {}).get('drive') or {}).get('id') or ''
    token = (bundle or {}).get('drive_token') or ''
    if not suite.check('a test voucher without a drive is 201 with a new drive',
                       status == 201 and drive_id.startswith('d_') and bool(token),
                       '(HTTP %d %r %s)' % (status, error_code(bundle), text[:120])):
        return
    before = unix_of(bundle.get('period_until'))
    status, value, _ = client.call('POST', '/v1/vouchers/redeem',
                                   {'code': TEST_EUR10, 'drive_id': drive_id}, bearer=token)
    days = (value or {}).get('days_added')
    after = unix_of((value or {}).get('period_until'))
    suite.check('a test voucher on the drive is 200 with the days it added (its value pro rata)',
                status == 200 and isinstance(days, int) and days > 30
                and (value or {}).get('months_added') == days // 30
                and after is not None and before is not None and after > before,
                '(HTTP %d %r, %r days)' % (status, error_code(value), days))
    status, value, _ = client.call('POST', '/v1/vouchers/redeem',
                                   {'code': 'AZLIN-NOT-A-CODE', 'drive_id': drive_id},
                                   bearer=token)
    suite.check('an unknown voucher is 400 voucher_invalid',
                status == 400 and error_code(value) == 'voucher_invalid',
                '(HTTP %d %r)' % (status, error_code(value)))


def recovery_checks(suite, client):
    """12. A recovery-key lockdown: registered, signed without a drive token, pending, cancelled
    by the owner."""
    status, bundle, text = client.signup('azlin-conformance-recovery')
    if not suite.check('a drive for the recovery key', status == 201 and isinstance(bundle, dict),
                       '(HTTP %d %s)' % (status, text[:120])):
        return
    drive_id, _, _, _ = azlin_client.bundle_drive(bundle)
    owner = bundle.get('drive_token') or ''
    path = '/v1/drives/%s' % drive_id
    secret, public = azlin_ed25519.new_key()
    status, value, _ = client.call('POST', path + '/recovery', {'recovery_pubkey': public},
                                   bearer=owner)
    suite.check("the owner registers the drive's recovery key", status == 200,
                '(HTTP %d %r)' % (status, error_code(value)))
    nonce = os.urandom(16).hex()
    message = ('lockdown:%s:%s' % (drive_id, nonce)).encode('utf-8')
    request = {'nonce': nonce, 'signature': azlin_ed25519.sign_b64(secret, message)}
    status, value, _ = client.call('POST', path + '/lockdown', request)
    pending_token = (value or {}).get('drive_token') or ''
    suite.check('a lockdown signed with the recovery key, without a drive token, is 202 pending',
                status == 202 and unix_of((value or {}).get('pending_until')) is not None
                and bool(pending_token), '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown', request)
    suite.check('the same lockdown request again is 409 nonce_used',
                status == 409 and error_code(value) == 'nonce_used',
                '(HTTP %d %r)' % (status, error_code(value)))
    other, _ = azlin_ed25519.new_key()
    nonce = os.urandom(16).hex()
    message = ('lockdown:%s:%s' % (drive_id, nonce)).encode('utf-8')
    status, value, _ = client.call('POST', path + '/lockdown',
                                   {'nonce': nonce,
                                    'signature': azlin_ed25519.sign_b64(other, message)})
    suite.check('a lockdown signed by another key is 401', status == 401,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=owner)
    suite.check("the drive's status names the pending lockdown",
                status == 200 and unix_of((value or {}).get('lockdown_pending_until')) is not None,
                '(HTTP %d %r)' % (status, (value or {}).get('lockdown_pending_until')))
    status, value, _ = client.call('POST', path + '/credentials', {}, bearer=pending_token)
    suite.check('the pending family gets no credentials before the notice ends (403 '
                'lockdown_pending)',
                status == 403 and error_code(value) == 'lockdown_pending',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown/cancel', {}, bearer=pending_token)
    suite.check('the pending family cannot cancel its own lockdown (403)', status == 403,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown/cancel', {}, bearer=owner)
    suite.check('the owner cancels the pending lockdown',
                status == 200 and (value or {}).get('cancelled') is True,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=owner)
    suite.check('then no lockdown is pending',
                status == 200 and (value or {}).get('lockdown_pending_until') is None,
                '(HTTP %d %r)' % (status, (value or {}).get('lockdown_pending_until')))
    status, value, _ = client.call('POST', path + '/lockdown/cancel', {}, bearer=owner)
    suite.check('nothing left to cancel is 409 no_pending_lockdown',
                status == 409 and error_code(value) == 'no_pending_lockdown',
                '(HTTP %d %r)' % (status, error_code(value)))


def claim_checks(suite, client):
    """9. The claim of a paid drive: the sign-up sealed to the checkout's claim key."""
    order = {'tier': '100GB', 'months': 3, 'method': 'card'}
    status, value, _ = client.call('POST', '/v1/checkout', order)
    suite.check('a checkout without a claim key is 400 claim_key_required',
                status == 400 and error_code(value) == 'claim_key_required',
                '(HTTP %d %r)' % (status, value))
    secret, claim_key = azlin_claim.new_claim_key()
    status, value, text = client.call('POST', '/v1/checkout', dict(order, claim_key=claim_key))
    checkout_id = (value or {}).get('checkout_id') or ''
    if not suite.check('a checkout with a claim key is 201 with its id and payment page',
                       status == 201 and bool(checkout_id) and bool((value or {}).get('pay_url')),
                       '(HTTP %d %s)' % (status, text[:200])):
        return
    path = '/v1/checkout/' + checkout_id
    status, value, _ = client.call('POST', path + '/pay', {'card_number': APPROVING_CARD})
    # The test provider may approve a moment later (`approves_in_secs`).
    deadline = time.time() + 60
    answer = {}
    while time.time() < deadline:
        status, answer, _ = client.call('GET', path)
        if (answer or {}).get('status') != 'pending':
            break
        time.sleep(1)
    answer = answer or {}
    sealed = answer.get('sealed_signup') or ''
    suite.check('the paid checkout is approved with a sealed sign-up and no plaintext one',
                answer.get('status') == 'approved' and bool(sealed) and 'signup' not in answer,
                '(status %r, keys %s)' % (answer.get('status'), sorted(answer)))
    try:
        bundle = json.loads(azlin_claim.open_sealed(sealed, secret, checkout_id))
    except ValueError as e:
        bundle = None
        suite.check('the sealed sign-up opens with the claim secret', False, '(%s)' % e)
    if bundle is not None:
        drive_id = ((bundle.get('drive') or {}).get('id')) or ''
        suite.check('the sealed sign-up opens with the claim secret to a drive bundle',
                    drive_id.startswith('d_') and TOKEN.match(bundle.get('drive_token') or '')
                    is not None, '(drive %r)' % drive_id)
    try:
        azlin_claim.open_sealed(sealed, secret, checkout_id + 'x')
        suite.check('the sealed sign-up opens for no other checkout id', False)
    except ValueError:
        suite.check('the sealed sign-up opens for no other checkout id', True)
    _, again, _ = client.call('GET', path)
    suite.check('a second poll answers the sealed sign-up again (kept, not deleted on read)',
                (again or {}).get('sealed_signup') == sealed)
    status, value, _ = client.call('GET', '/v1/checkout/ck_' + 'a' * 26)
    suite.check('an unknown checkout is 404', status == 404, '(HTTP %d %r)' % (status, value))
    if bundle is not None:
        period_checks(suite, client, checkout_id, order, bundle)


def period_checks(suite, client, checkout_id, order, bundle):
    """10. The period tokens of the paid checkout: issued against the sealed sign-up's issue key
    only, finalized, redeemed once."""
    grant = bundle.get('period_tokens') or {}
    issue_key = grant.get('issue_key') or ''
    months, tier = order['months'], order['tier']
    if not suite.check("the sealed sign-up grants the checkout's months against an issue key",
                       grant.get('checkout_id') == checkout_id and grant.get('months') == months
                       and len(issue_key) == 43, '(keys %s, months %r)' % (sorted(grant),
                                                                        grant.get('months'))):
        return
    status, value, _ = client.call('GET', '/v1/tokens/keys')
    keys = {k.get('tier'): k for k in ((value or {}).get('keys') or []) if isinstance(k, dict)}
    key = keys.get(tier) or {}
    try:
        n, e = azlin_period.parse_public_key_pem(key.get('public_key_pem') or '')
    except ValueError as err:
        suite.check("GET /v1/tokens/keys names the tier's issuer key", False, '(%s)' % err)
        return
    suite.check("GET /v1/tokens/keys names the tier's issuer key (RSA, 2048 bits or more)",
                status == 200 and key.get('key_id') == '%s/%s' % (tier, key.get('year'))
                and n.bit_length() >= 2048, '(HTTP %d, %r)' % (status, key.get('key_id')))
    blindings = [azlin_period.blind(n, e, tier, key['year']) for _ in range(months)]
    request = {'checkout_id': checkout_id, 'blinded': [blinded for _, blinded in blindings]}
    status, value, _ = client.call('POST', '/v1/tokens/issue', request)
    suite.check('period tokens without the issue key are 400 issue_key_required',
                status == 400 and error_code(value) == 'issue_key_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', '/v1/tokens/issue',
                                   dict(request, issue_key=azlin_period.new_issue_key()[0],
                                        key_id=key.get('key_id')))
    suite.check('period tokens with another issue key are 403 issue_key_wrong',
                status == 403 and error_code(value) == 'issue_key_wrong',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', '/v1/tokens/issue', dict(request, issue_key=issue_key))
    suite.check('period tokens without the key id they were blinded for are 400 key_id_required',
                status == 400 and error_code(value) == 'key_id_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', '/v1/tokens/issue',
                                   dict(request, issue_key=issue_key, key_id='%s/2000' % tier))
    suite.check('period tokens for a key the server no longer signs with are 409 key_changed '
                'naming the current one',
                status == 409 and error_code(value) == 'key_changed'
                and (value or {}).get('key_id') == key.get('key_id'),
                '(HTTP %d %r %r)' % (status, error_code(value), (value or {}).get('key_id')))
    request = dict(request, issue_key=issue_key, key_id=key.get('key_id'))
    status, value, _ = client.call('POST', '/v1/tokens/issue', request)
    signatures = (value or {}).get('blind_signatures') or []
    if not suite.check('period tokens with the issue key and the key id are 200, one blind '
                       'signature a month (nothing was counted for key_changed)',
                       status == 200 and len(signatures) == months
                       and (value or {}).get('key_id') == key.get('key_id'),
                       '(HTTP %d %r, %d signatures)' % (status, error_code(value),
                                                         len(signatures))):
        return
    status, again, _ = client.call('POST', '/v1/tokens/issue', request)
    suite.check('the identical request again (its answer lost) answers the same signatures',
                status == 200 and (again or {}).get('blind_signatures') == signatures,
                '(HTTP %d %r)' % (status, error_code(again)))
    try:
        tokens = [azlin_period.finalize(n, e, state, signature)
                  for (state, _), signature in zip(blindings, signatures)]
        suite.check('every blind signature finalizes into a token the issuer key verifies', True)
    except ValueError as err:
        suite.check('every blind signature finalizes into a token the issuer key verifies', False,
                    '(%s)' % err)
        return
    one_more = azlin_period.blind(n, e, tier, key['year'])[1]
    status, value, _ = client.call('POST', '/v1/tokens/issue',
                                   {'checkout_id': checkout_id, 'blinded': [one_more],
                                    'issue_key': issue_key, 'key_id': key.get('key_id')})
    suite.check('a token past the paid months (counted once) is 409 already_issued',
                status == 409 and error_code(value) == 'already_issued',
                '(HTTP %d %r)' % (status, error_code(value)))
    drive_id = (bundle.get('drive') or {}).get('id') or ''
    path = '/v1/drives/%s/redeem' % drive_id
    status, value, _ = client.call('POST', path, tokens[0], bearer=bundle.get('drive_token'))
    until = unix_of((value or {}).get('period_until'))
    before = unix_of(bundle.get('period_until'))
    suite.check('a period token redeemed with the drive token makes the period longer',
                status == 200 and until is not None and before is not None and until > before,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path, tokens[0], bearer=bundle.get('drive_token'))
    suite.check('the same period token again is 409 token_used',
                status == 409 and error_code(value) == 'token_used',
                '(HTTP %d %r)' % (status, error_code(value)))
    previous_token_checks(suite, client, drive_id, bundle.get('drive_token') or '', tokens[1])


def previous_token_checks(suite, client, drive_id, first, token):
    """11. The token just replaced still reads: a redemption and the drive's info with the token
    a refresh rotated from, without revoking the family."""
    status, renewed, _ = client.refresh(drive_id, first)
    second = (renewed or {}).get('drive_token') or ''
    if not suite.check('the paid drive refreshes with its first token', status == 200 and second,
                       '(HTTP %d %r)' % (status, error_code(renewed))):
        return
    path = '/v1/drives/%s' % drive_id
    status, value, _ = client.call('POST', path + '/redeem', token, bearer=first)
    suite.check('the previous drive token redeems a period token',
                status == 200 and unix_of((value or {}).get('period_until')) is not None,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=first)
    suite.check("the previous drive token reads the drive's period",
                status == 200 and unix_of((value or {}).get('period_until')) is not None,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, renewed, _ = client.refresh(drive_id, second)
    third = (renewed or {}).get('drive_token') or ''
    suite.check('and the family is not revoked: the new token refreshes',
                status == 200 and bool(third), '(HTTP %d %r)' % (status, error_code(renewed)))
    status, value, _ = client.call('GET', path, bearer=first)
    suite.check('a token older than the previous one is a reuse on a read too (401 token_reuse)',
                status == 401 and error_code(value) == 'token_reuse',
                '(HTTP %d %r)' % (status, error_code(value)))


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--token-url')
    parser.add_argument('--s3-url', help="reach the bucket here instead of the bundle's endpoint")
    parser.add_argument('--mock', action='store_true',
                        help='start scripts/azlin_mock_stack.py on free ports and check it')
    parser.add_argument('--skip-vouchers', action='store_true',
                        help='skip section 13 (vouchers with the test codes)')
    parser.add_argument('--cash-key', help="the server's cash key (hex), for the activation "
                                           "code's MAC check (the mock's is known)")
    args = parser.parse_args()
    cash_key = bytes.fromhex(args.cash_key) if args.cash_key else None
    if args.mock:
        import azlin_mock_stack  # noqa: PLC0415 - only for --mock
        root = tempfile.mkdtemp(prefix='azlin-conformance-')
        stack = azlin_mock_stack.start(root)
        try:
            failures = run(stack.token_url,
                           vouchers='skip' if args.skip_vouchers else 'required',
                           operator=MockOperator(stack.token.state),
                           cash_key=cash_key or azlin_cash.MOCK_KEY)
        finally:
            stack.stop()
    else:
        failures = run(azlin_client.token_url_from(args.token_url), args.s3_url,
                       vouchers='skip' if args.skip_vouchers else 'auto', cash_key=cash_key)
    print('PASS' if failures == 0 else 'FAIL: %d check(s)' % failures, flush=True)
    sys.exit(min(failures, 100))


if __name__ == '__main__':
    main()
