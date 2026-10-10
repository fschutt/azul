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
12. The recovery key (§18.7, AZLINSEC17 F14, scripts/azlin_ed25519.py): the signup answer
    names `lockdown_pending_until`; a new drive's owner registers its first Ed25519 key on the
    token alone (POST /v1/drives/<id>/recovery: `ok`, `key_id`), replacing it unsigned is 403
    `recovery_key_required`; a lockdown signed with it and no drive token is 202 with
    `pending_until` and a new family's drive token; the same request again is 409 `nonce_used`,
    one signed by another key 401; the drive's status names the pending lockdown. The pending
    family (D42): its status is `{"id", "status": "lockdown_pending", "lockdown_pending_until",
    "you"}` only, and every drive-token route - credentials, members, keys (POST, DELETE), a
    lockdown by token, recovery, recovery_keys (GET, POST, DELETE), restore and its status,
    redeem, vouchers/redeem with a drive - is 403 `lockdown_pending` with `pending_until`, the
    token not rotated. The owner's device lockdown meanwhile is 200 and leaves the pending
    recovery and its 48 hours alone (F12); a device token cannot cancel it (403
    `recovery_key_required`), another key's signature is 401, the recovery key's over
    `lockdown-cancel:<drive>:<nonce>` without a token calls it off (the pending family then
    401), and then there is none to cancel (409 `no_pending_lockdown`).
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

16. Several recovery keys (D51, F12 option C): a drive signed up with `recovery_pubkey` lists
    it (GET /v1/drives/<id>/recovery_keys: `key_id`, `label` "recovery code",
    `recovery_pubkey`, `created_at`, `verified`); adding one needs a current key's signature
    over `recovery-add:<drive>:<recovery_pubkey>:<nonce>` (403 `recovery_key_required`
    without, 201 with), a key it has already is 409 `recovery_key_exists`; a lockdown signed by
    the second key (`key_id`) is 202 and the first calls it off; removing (DELETE
    .../recovery_keys/<key_id>, `recovery-remove:<drive>:<key_id>:<nonce>`) an unknown key is
    404 `no_such_key`, unsigned 403, signed 200, the last key 409 `last_recovery_key`; POST
    .../recovery signed over `recovery:<drive>:<new key>:<nonce>` replaces every key; ten keys
    at most (409 `too_many_recovery_keys`).
17. The lookup by recovery key (§18.8): POST /v1/recovery/challenge is `rc1.<expires>.<random>.
    <mac>` with its expiry; POST /v1/recovery/lookup {"recovery_pubkey", "challenge",
    "signature" over `recovery-lookup:<challenge>`} names the drives the key belongs to (`drive_id`,
    `key_id`), none for a key nobody registered; a forged or expired challenge is 401
    `bad_challenge`, a bad signature 401 `unauthorized`. The mock only: 20 recovery requests per
    10 minutes per address, then 429 `rate_limited` (against a server it would shut the address
    out).
18. The mock only (its clock advanced): the owner's refresh during a recovery-key lockdown names
    `lockdown_pending_until`; 48 hours later the pending family's first refresh is 200 and hands
    it the drive, the owner's old devices 401.

Every drive token, claim secret and issue key is secret: none is printed.
"""
import argparse
import base64
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


def rfc3339(unix):
    """Seconds since 1970 as RFC 3339 (`2026-11-07T09:15:00Z`)."""
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(unix))


# A development token server's test vouchers (azlin-proto's voucher module).
TEST_ONE_MONTH = 'AZLIN-TEST-1M'
TEST_EUR10 = 'AZLIN-TEST-EUR10'


def run(token_url, s3_url=None, vouchers='auto', advance=None):
    """`vouchers`: 'auto' (section 13 unless the server takes no test code), 'required' (the
    mock: never skipped), 'skip'. `advance(secs)`: the mock's clock (sections 17's rate limit and
    18 run only with it)."""
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
    registered = recovery_key_checks(suite, client)
    if registered:
        lookup_checks(suite, client, registered, mock=advance is not None)
    if advance is not None:
        handover_checks(suite, client, advance)
    lockdown_checks(suite, client)
    restore_checks(suite, client, s3_url)
    if vouchers == 'skip':
        print('skipped: vouchers (--skip-vouchers)', flush=True)
    else:
        voucher_checks(suite, client, required=vouchers == 'required')
    return suite.failures


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


def canonical_key(public):
    """An Ed25519 public key as the token server lists it: standard base64 with padding."""
    raw = base64.b64decode(public + '=' * (-len(public) % 4))
    return base64.b64encode(raw).decode('ascii')


def signed(secret, what, key_id=None, **fields):
    """`fields` with a fresh nonce and the recovery key's signature over `<what>:<nonce>`."""
    nonce = os.urandom(16).hex()
    body = dict(fields, nonce=nonce,
                signature=azlin_ed25519.sign_b64(secret, ('%s:%s' % (what, nonce)).encode('utf-8')))
    if key_id:
        body['key_id'] = key_id
    return body


def recovery_checks(suite, client):
    """12. A recovery-key lockdown: registered, signed without a drive token, pending (the
    pending family refused everywhere), a device lockdown leaving it alone, cancelled by a
    recovery key (F12)."""
    status, bundle, text = client.signup('azlin-conformance-recovery')
    if not suite.check('a drive for the recovery key', status == 201 and isinstance(bundle, dict),
                       '(HTTP %d %s)' % (status, text[:120])):
        return
    suite.check('the signup answer names lockdown_pending_until (none)',
                'lockdown_pending_until' in bundle
                and bundle.get('lockdown_pending_until') is None)
    drive_id, _, _, _ = azlin_client.bundle_drive(bundle)
    owner = bundle.get('drive_token') or ''
    path = '/v1/drives/%s' % drive_id
    secret, public = azlin_ed25519.new_key()
    status, value, _ = client.call('POST', path + '/recovery', {'recovery_pubkey': public},
                                   bearer=owner)
    suite.check("the owner registers the drive's first recovery key on the token alone",
                status == 200 and (value or {}).get('ok') is True
                and bool((value or {}).get('key_id')),
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/recovery',
                                   {'recovery_pubkey': azlin_ed25519.new_key()[1]}, bearer=owner)
    suite.check('replacing it without a recovery key signature is 403 recovery_key_required',
                status == 403 and error_code(value) == 'recovery_key_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    request = signed(secret, 'lockdown:%s' % drive_id)
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
    status, value, _ = client.call('POST', path + '/lockdown',
                                   signed(other, 'lockdown:%s' % drive_id))
    suite.check('a lockdown signed by another key is 401', status == 401,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=owner)
    suite.check("the drive's status names the pending lockdown",
                status == 200 and unix_of((value or {}).get('lockdown_pending_until')) is not None,
                '(HTTP %d %r)' % (status, (value or {}).get('lockdown_pending_until')))
    # D42: the pending family gets nothing before the 48 hours are over.
    status, value, _ = client.call('GET', path, bearer=pending_token)
    value = value or {}
    suite.check("the pending family's status is its lockdown's only",
                status == 200 and value.get('status') == 'lockdown_pending'
                and unix_of(value.get('lockdown_pending_until')) is not None
                and value.get('you') == 'recovery-pending'
                and 'tier' not in value and 'members' not in value,
                '(HTTP %d %r)' % (status, sorted(value)))
    routes = [
        ('POST', path + '/credentials', {}),
        ('POST', path + '/members', {}),
        ('POST', path + '/keys', {}),
        ('DELETE', path + '/keys/AZKNOTAKEY', {}),
        ('POST', path + '/lockdown', {}),
        ('POST', path + '/recovery', {'recovery_pubkey': public}),
        ('GET', path + '/recovery_keys', None),
        ('POST', path + '/recovery_keys', {'recovery_pubkey': public}),
        ('DELETE', path + '/recovery_keys/rk_legacy', {}),
        ('POST', path + '/restore', {'prefix': '', 'as_of': rfc3339(int(time.time()))}),
        ('GET', path + '/restore/r_nothing', None),
        ('POST', path + '/redeem', {}),
        ('POST', '/v1/vouchers/redeem', {'code': 'AZLIN-TEST-1M', 'drive_id': drive_id}),
    ]
    for method, route, body in routes:
        status, value, _ = client.call(method, route, body, bearer=pending_token)
        suite.check('the pending family: %s %s is 403 lockdown_pending with pending_until'
                    % (method, route.replace(drive_id, '<id>')),
                    status == 403 and error_code(value) == 'lockdown_pending'
                    and unix_of((value or {}).get('pending_until')) is not None,
                    '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/credentials', {}, bearer=pending_token)
    suite.check('the refused token was not rotated: it is refused the same way again',
                status == 403 and error_code(value) == 'lockdown_pending',
                '(HTTP %d %r)' % (status, error_code(value)))
    # A device's lockdown leaves a pending recovery alone (F12: the recovery key wins).
    status, value, _ = client.call('POST', path + '/lockdown', {}, bearer=owner)
    owner = (value or {}).get('drive_token') or owner
    suite.check("the owner's device lockdown during it is 200 with a new token",
                status == 200 and bool((value or {}).get('drive_token')),
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/credentials', {}, bearer=pending_token)
    suite.check('the pending recovery survives the device lockdown',
                status == 403 and error_code(value) == 'lockdown_pending',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=owner)
    suite.check('and its 48 hours still run',
                status == 200 and unix_of((value or {}).get('lockdown_pending_until')) is not None,
                '(HTTP %d %r)' % (status, (value or {}).get('lockdown_pending_until')))
    # F12: only a recovery key calls it off - no drive token.
    status, value, _ = client.call('POST', path + '/lockdown/cancel', {}, bearer=owner)
    suite.check("a device's token cannot cancel it (403 recovery_key_required)",
                status == 403 and error_code(value) == 'recovery_key_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown/cancel',
                                   signed(other, 'lockdown-cancel:%s' % drive_id))
    suite.check('a cancel signed by another key is 401', status == 401,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown/cancel',
                                   signed(secret, 'lockdown-cancel:%s' % drive_id))
    suite.check('a cancel signed by the recovery key, without a token, calls it off',
                status == 200 and (value or {}).get('cancelled') is True,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path, bearer=owner)
    suite.check('then no lockdown is pending',
                status == 200 and (value or {}).get('lockdown_pending_until') is None,
                '(HTTP %d %r)' % (status, (value or {}).get('lockdown_pending_until')))
    status, value, _ = client.call('POST', path + '/credentials', {}, bearer=pending_token)
    suite.check('and the pending family is gone (401)', status == 401,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown/cancel',
                                   signed(secret, 'lockdown-cancel:%s' % drive_id))
    suite.check('nothing left to cancel is 409 no_pending_lockdown',
                status == 409 and error_code(value) == 'no_pending_lockdown',
                '(HTTP %d %r)' % (status, error_code(value)))


def recovery_key_checks(suite, client):
    """16. Several recovery keys (D51, F12 option C): listed, each change signed by a current
    key besides the drive token, never the last one removed, at most ten."""
    first_secret, first = azlin_ed25519.new_key()
    status, bundle, text = client.call('POST', '/v1/drives',
                                       {'name': 'azlin-conformance-keys', 'tier': '100GB',
                                        'recovery_pubkey': first})
    if not suite.check('a drive signed up with its recovery key', status == 201
                       and isinstance(bundle, dict), '(HTTP %d %s)' % (status, text[:120])):
        return
    drive_id, _, _, _ = azlin_client.bundle_drive(bundle)
    owner = bundle.get('drive_token') or ''
    path = '/v1/drives/%s' % drive_id
    status, value, _ = client.call('GET', path + '/recovery_keys', bearer=owner)
    keys = (value or {}).get('keys') or []
    suite.check("GET recovery_keys lists the sign-up's key, verified, as the recovery code",
                status == 200 and len(keys) == 1
                and keys[0].get('recovery_pubkey') == canonical_key(first)
                and keys[0].get('label') == 'recovery code' and keys[0].get('verified') is True
                and bool(keys[0].get('key_id')) and 'created_at' in keys[0],
                '(HTTP %d %r)' % (status, keys))
    first_id = keys[0].get('key_id') if keys else None
    second_secret, second = azlin_ed25519.new_key()
    status, value, _ = client.call('POST', path + '/recovery_keys',
                                   {'recovery_pubkey': second, 'label': 'second kit'},
                                   bearer=owner)
    suite.check('adding a key on the token alone is 403 recovery_key_required',
                status == 403 and error_code(value) == 'recovery_key_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    add = signed(first_secret, 'recovery-add:%s:%s' % (drive_id, second),
                 recovery_pubkey=second, label='second kit')
    status, value, _ = client.call('POST', path + '/recovery_keys', add, bearer=owner)
    second_id = (value or {}).get('key_id')
    suite.check('adding one signed by a current key is 201 with the key',
                status == 201 and str(second_id or '').startswith('rk_')
                and (value or {}).get('label') == 'second kit'
                and (value or {}).get('recovery_pubkey') == canonical_key(second),
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/recovery_keys', add, bearer=owner)
    suite.check('the same signed request again is 409 (recovery_key_exists or nonce_used)',
                status == 409, '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/recovery_keys',
                                   signed(first_secret, 'recovery-add:%s:%s' % (drive_id, second),
                                          recovery_pubkey=second), bearer=owner)
    suite.check('a key the drive has already is 409 recovery_key_exists',
                status == 409 and error_code(value) == 'recovery_key_exists',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path + '/recovery_keys', bearer=owner)
    suite.check('both keys are listed',
                status == 200 and len((value or {}).get('keys') or []) == 2,
                '(HTTP %d %r)' % (status, value))
    status, value, _ = client.call('POST', path + '/lockdown',
                                   signed(second_secret, 'lockdown:%s' % drive_id,
                                          key_id=second_id))
    suite.check('a lockdown signed by the second key is 202 pending', status == 202,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', path + '/lockdown/cancel',
                                   signed(first_secret, 'lockdown-cancel:%s' % drive_id))
    suite.check('the first key calls it off', status == 200,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('DELETE', path + '/recovery_keys/rk_nobody',
                                   signed(first_secret, 'recovery-remove:%s:rk_nobody'
                                          % drive_id), bearer=owner)
    suite.check('removing an unknown key is 404 no_such_key',
                status == 404 and error_code(value) == 'no_such_key',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('DELETE', path + '/recovery_keys/%s' % second_id, {},
                                   bearer=owner)
    suite.check('removing one on the token alone is 403 recovery_key_required',
                status == 403 and error_code(value) == 'recovery_key_required',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('DELETE', path + '/recovery_keys/%s' % second_id,
                                   signed(first_secret, 'recovery-remove:%s:%s'
                                          % (drive_id, second_id)), bearer=owner)
    suite.check('removing the second key signed by the first is 200',
                status == 200 and (value or {}).get('removed') == second_id,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('DELETE', path + '/recovery_keys/%s' % first_id,
                                   signed(first_secret, 'recovery-remove:%s:%s'
                                          % (drive_id, first_id)), bearer=owner)
    suite.check("the drive's last key stays: 409 last_recovery_key",
                status == 409 and error_code(value) == 'last_recovery_key',
                '(HTTP %d %r)' % (status, error_code(value)))
    third_secret, third = azlin_ed25519.new_key()
    status, value, _ = client.call('POST', path + '/recovery',
                                   signed(first_secret, 'recovery:%s:%s' % (drive_id, third),
                                          recovery_pubkey=third), bearer=owner)
    suite.check('POST recovery signed by a current key replaces every key',
                status == 200 and (value or {}).get('ok') is True
                and bool((value or {}).get('key_id')),
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('GET', path + '/recovery_keys', bearer=owner)
    keys = (value or {}).get('keys') or []
    suite.check('then the new key is the only one',
                status == 200 and [k.get('recovery_pubkey') for k in keys]
                == [canonical_key(third)], '(HTTP %d %r)' % (status, keys))
    added = 1
    for _ in range(9):
        _, public = azlin_ed25519.new_key()
        status, value, _ = client.call('POST', path + '/recovery_keys',
                                       signed(third_secret, 'recovery-add:%s:%s'
                                              % (drive_id, public), recovery_pubkey=public),
                                       bearer=owner)
        added += status == 201
    _, public = azlin_ed25519.new_key()
    status, value, _ = client.call('POST', path + '/recovery_keys',
                                   signed(third_secret, 'recovery-add:%s:%s' % (drive_id, public),
                                          recovery_pubkey=public), bearer=owner)
    suite.check('ten keys at most: the eleventh is 409 too_many_recovery_keys',
                added == 10 and status == 409 and error_code(value) == 'too_many_recovery_keys',
                '(%d added, HTTP %d %r)' % (added, status, error_code(value)))
    return drive_id, third_secret, third


def lookup_checks(suite, client, registered, mock):
    """17. The lookup by recovery key (§18.8): a computer that never had the drive finds its id
    from the kit's code - a challenge, signed - and nobody else learns whose a key is."""
    drive_id, secret, public = registered
    status, value, _ = client.call('POST', '/v1/recovery/challenge', {})
    challenge = (value or {}).get('challenge') or ''
    suite.check('POST /v1/recovery/challenge is 200 with an rc1 challenge and its expiry',
                status == 200 and challenge.startswith('rc1.') and len(challenge.split('.')) == 4
                and unix_of((value or {}).get('expires_at')) is not None,
                '(HTTP %d %r)' % (status, value))

    def lookup(key_secret, key_public, text):
        sig = azlin_ed25519.sign_b64(key_secret, ('recovery-lookup:%s' % text).encode('utf-8'))
        return client.call('POST', '/v1/recovery/lookup',
                           {'recovery_pubkey': key_public, 'challenge': text, 'signature': sig})

    status, value, _ = lookup(secret, public, challenge)
    drives = (value or {}).get('drives') or []
    suite.check("a lookup signed by a drive's recovery key names the drive and the key",
                status == 200 and any(d.get('drive_id') == drive_id
                                      and str(d.get('key_id') or '').startswith('rk_')
                                      for d in drives),
                '(HTTP %d %r)' % (status, error_code(value)))
    stranger_secret, stranger = azlin_ed25519.new_key()
    status, value, _ = lookup(stranger_secret, stranger, challenge)
    suite.check("an unregistered key's lookup is 200 with no drive",
                status == 200 and (value or {}).get('drives') == [],
                '(HTTP %d %r)' % (status, value))
    forged = challenge[:-4] + ('AAAA' if not challenge.endswith('AAAA') else 'BBBB')
    status, value, _ = lookup(secret, public, forged)
    suite.check('a forged challenge is 401 bad_challenge',
                status == 401 and error_code(value) == 'bad_challenge',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = lookup(secret, public, 'rc1.1.x.y')
    suite.check('an expired one too', status == 401 and error_code(value) == 'bad_challenge',
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.call('POST', '/v1/recovery/lookup',
                                   {'recovery_pubkey': public, 'challenge': challenge,
                                    'signature': azlin_ed25519.sign_b64(stranger_secret,
                                                                        b'something else')})
    suite.check('a bad signature is 401 unauthorized',
                status == 401 and error_code(value) == 'unauthorized',
                '(HTTP %d %r)' % (status, error_code(value)))
    if not mock:
        print('skipped: the lookup rate limit (it would shut this address out for 10 minutes)',
              flush=True)
        return
    # Six recovery requests so far from this address; the window takes 20.
    statuses = [client.call('POST', '/v1/recovery/challenge', {})[0] for _ in range(21)]
    suite.check('the mock: 20 recovery requests per 10 minutes per address, then 429',
                statuses == [200] * 14 + [429] * 7, '(%r)' % statuses)


def handover_checks(suite, client, advance):
    """18. (the mock, its clock advanced) The first refresh after the 48 hours hands the drive
    over: 200 for the pending family, the owner's devices refused."""
    secret, public = azlin_ed25519.new_key()
    status, bundle, _ = client.call('POST', '/v1/drives', {'name': 'azlin-conformance-handover',
                                                           'recovery_pubkey': public})
    if not suite.check('a drive for the hand-over', status == 201):
        return
    drive_id, _, _, _ = azlin_client.bundle_drive(bundle)
    owner = bundle.get('drive_token') or ''
    path = '/v1/drives/%s' % drive_id
    status, value, _ = client.call('POST', path + '/lockdown', signed(secret, 'lockdown:%s'
                                                                      % drive_id))
    pending_token = (value or {}).get('drive_token') or ''
    status, value, _ = client.refresh(drive_id, owner)
    owner = (value or {}).get('drive_token') or owner
    suite.check("the owner's refresh during it names lockdown_pending_until",
                status == 200 and unix_of((value or {}).get('lockdown_pending_until')) is not None,
                '(HTTP %d %r)' % (status, (value or {}).get('lockdown_pending_until')))
    advance(48 * 3600 + 60)
    status, value, _ = client.refresh(drive_id, pending_token)
    suite.check('48 hours later the pending family\'s refresh hands it the drive (200)',
                status == 200 and bool((value or {}).get('credentials'))
                and (value or {}).get('lockdown_pending_until') is None,
                '(HTTP %d %r)' % (status, error_code(value)))
    status, value, _ = client.refresh(drive_id, owner)
    suite.check("and the owner's old devices are refused (401)", status == 401,
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
    args = parser.parse_args()
    if args.mock:
        import azlin_mock_stack  # noqa: PLC0415 - only for --mock
        root = tempfile.mkdtemp(prefix='azlin-conformance-')
        stack = azlin_mock_stack.start(root)
        try:
            failures = run(stack.token_url,
                           vouchers='skip' if args.skip_vouchers else 'required',
                           advance=stack.token.state.advance)
        finally:
            stack.stop()
    else:
        failures = run(azlin_client.token_url_from(args.token_url), args.s3_url,
                       vouchers='skip' if args.skip_vouchers else 'auto')
    print('PASS' if failures == 0 else 'FAIL: %d check(s)' % failures, flush=True)
    sys.exit(min(failures, 100))


if __name__ == '__main__':
    main()
