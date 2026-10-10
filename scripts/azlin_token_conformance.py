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


def run(token_url, s3_url=None):
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
    return suite.failures


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
    args = parser.parse_args()
    if args.mock:
        import azlin_mock_stack  # noqa: PLC0415 - only for --mock
        root = tempfile.mkdtemp(prefix='azlin-conformance-')
        stack = azlin_mock_stack.start(root)
        try:
            failures = run(stack.token_url)
        finally:
            stack.stop()
    else:
        failures = run(azlin_client.token_url_from(args.token_url), args.s3_url)
    print('PASS' if failures == 0 else 'FAIL: %d check(s)' % failures, flush=True)
    sys.exit(min(failures, 100))


if __name__ == '__main__':
    main()
