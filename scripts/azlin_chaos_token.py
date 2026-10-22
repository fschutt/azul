#!/usr/bin/env python3
"""The token server's and the node's answers under faults, over HTTP: what a client meets when an
answer is lost after the server acted, and the node's own quota. A fault proxy
(scripts/azchaos.py) stands in front of the token server and one in front of the S3; the checks
talk through them as an app does, and steer them to lose or replace one answer.

    python3 scripts/azlin_chaos_token.py --mock
        the mock stack (scripts/azlin_mock_stack.py) on free ports, its fake providers on
    python3 scripts/azlin_chaos_token.py --token-url URL --s3-url URL --sandbox-url URL
            [--azctl PATH] [--env dev]
        a running stack: azul-apps local/fullsim.sh's native token server, its s3d and its
        payment sandbox (`local/e2e.sh --only chaos-token` runs it so); the bucket's quota is
        set with `azctl --env ENV --yes bucket set`

Scenarios (`--only NAME,...`, `--list`); each ends PASS or FAIL with what it saw, and a FAIL names
the unit of the logical refactoring that makes it pass:

  lost-refresh   a refresh whose answer is lost (the connection reset after the rotation), and one
                 whose client heard 503 after it: the same token again is answered with the
                 family alive, the new token refreshes, and the old one used once more after
                 that is still a reuse (401 token_reuse)
  lost-lockdown  "lock down from this device" whose answer is lost: the same device asks again
                 with the same idempotency key and gets a grant whose token refreshes
  lost-claim     a pick-up's claim whose 201 is lost, asked again twice with the same key: one
                 pick-up counted (claims_left 2), its token refreshes
  lost-redeem    a period token's redemption whose answer is lost: the same token again with
                 the same key is answered 200 (not token_used) and the period grew by one month
  paid-abandon   a card paid at the provider whose webhook is late, then abandoned (the popover
                 closed before the return page): the checkout is not expired (200 pending, or
                 409 checkout_paid), a lost abandon answer asked again says the same, and the
                 late webhook approves it with a sign-up the claim secret opens
  quota          the node's quota on a bucket: parts of an unfinished upload count (the part and
                 the PUT that pass it are 403 quota_exceeded), an aborted upload frees them, and
                 of two PUTs at once that each fit alone exactly one lands
  lost-complete  a CompleteMultipartUpload whose answer is lost: the object is there with the
                 multipart ETag the client computes (the MD5 of the parts' MD5s, a dash, their
                 count), so a client can verify it instead of uploading again

Every request to the token server says its hello (`x-azlin-client: azlin-chaos/1.0.0;
contract=<n>`) and, where the call spends or grants something, `x-azlin-idempotency` (the same
on a retry) - both as the contract crate (examples/azlin-contract) defines them. Drive tokens, claim secrets and issue
keys are secret: none is printed. Exit code: the number of failed scenarios.
"""
import argparse
import calendar
import hashlib
import http.client
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import azchaos  # noqa: E402
import azlin_claim  # noqa: E402
import azlin_client  # noqa: E402
import azlin_contract  # noqa: E402
import azlin_period  # noqa: E402

APPROVING_CARD = '4242 4242 4242 4242'
HELLO = azlin_contract.client_hello('azlin-chaos', '1.0.0')
MIB = 1024 * 1024
MONTH = 30 * 86400
# How long the late webhook may take once let go (the sandbox posts every quarter second, after
# the delay its switch set).
WEBHOOK_WAIT = 90
# How long a node may take to learn a bucket's new quota from the control state.
QUOTA_WAIT = 90
# The answers a client cannot tell from a lost one: no status at all.
LOST = (OSError, http.client.HTTPException)


class Failed(Exception):
    """A scenario's expectation that did not hold."""


def expect(condition, message):
    if not condition:
        raise Failed(message)


def idempotency_key():
    return azlin_contract.idempotency_key()


def code_of(value):
    return value.get('error') if isinstance(value, dict) else None


def family_of(token):
    return (token or '')[3:].split('.')[0] if (token or '').startswith('dt_') else None


def unix_of(text):
    if not isinstance(text, str):
        return None
    try:
        return calendar.timegm(time.strptime(text, '%Y-%m-%dT%H:%M:%SZ'))
    except ValueError:
        return None


# ---- the switches of the stack under test ----


class MockOperator:
    """The mock stack's switches."""

    card_provider = 'fake-stripe'

    def __init__(self, state, token_url):
        self.state = state
        self.token_url = token_url

    def set_quota(self, drive_id, bucket, quota):
        with self.state.lock:
            self.state.drives[drive_id]['quota_bytes'] = int(quota)

    def hold_webhooks(self, on):
        self.state.hold_webhooks(on)

    def confirm_card(self, client_secret):
        """The fake provider's fields page takes the approving card."""
        return post_json(self.token_url + '/fake-stripe/confirm',
                         {'client_secret': client_secret, 'card_number': APPROVING_CARD})


class StackOperator:
    """A local stack's switches: azctl for a bucket's quota, the payment sandbox's for its cards
    and its webhooks."""

    card_provider = 'fake-stripe'

    def __init__(self, azctl, env, sandbox_url):
        self.azctl = azctl
        self.env = env
        self.sandbox_url = (sandbox_url or '').rstrip('/')

    def set_quota(self, drive_id, bucket, quota):
        if not self.azctl:
            raise Failed('no azctl (--azctl) to set the quota of %s' % bucket)
        done = subprocess.run([self.azctl, '--env', self.env, '--yes', 'bucket', 'set', bucket,
                               '--quota', str(int(quota))], capture_output=True, text=True,
                              timeout=120, check=False)
        expect(done.returncode == 0, 'azctl bucket set %s: %s' % (bucket, done.stderr[-400:]))

    def hold_webhooks(self, on):
        if not self.sandbox_url:
            raise Failed('no payment sandbox (--sandbox-url) to hold the webhooks back')
        status, value = post_json(self.sandbox_url + '/_dev/delay', {'secs': 30 if on else 0},
                                  with_status=True)
        expect(status == 200, 'the sandbox took no delay: %s %r' % (status, value))

    def confirm_card(self, client_secret):
        return post_json(self.sandbox_url + '/stripe/confirm',
                         {'client_secret': client_secret, 'card_number': '4242424242424242',
                          'exp': '1234', 'cvc': '123', 'country': 'DE'})


def post_json(url, body, with_status=False):
    request = urllib.request.Request(url, data=json.dumps(body).encode('utf-8'), method='POST',
                                     headers={'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            status, raw = response.status, response.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    try:
        value = json.loads(raw.decode('utf-8') or 'null')
    except ValueError:
        value = None
    return (status, value) if with_status else value


# ---- the world of one run ----


class World:
    """The token server and the S3 under test, each behind a fault proxy."""

    def __init__(self, token_url, s3_url, operator):
        self.token_proxy = azchaos.ChaosProxy(token_url, kind='token').start()
        self.s3_url = s3_url
        self.s3_proxy = None
        self.client = azlin_client.TokenClient(self.token_proxy.url)
        self.operator = operator

    def stop(self):
        self.token_proxy.stop()
        if self.s3_proxy is not None:
            self.s3_proxy.stop()

    def clear(self):
        self.token_proxy.clear()
        self.token_proxy.release()
        if self.s3_proxy is not None:
            self.s3_proxy.clear()
            self.s3_proxy.release()

    def bucket(self, bundle):
        """The bundle's bucket through the S3 proxy (in front of --s3-url, else the bundle's
        endpoint)."""
        if self.s3_proxy is None:
            upstream = self.s3_url or azlin_client.bundle_drive(bundle)[2]
            self.s3_proxy = azchaos.ChaosProxy(upstream, kind='s3').start()
        return azlin_client.Bucket(bundle, endpoint=self.s3_proxy.url)

    def call(self, method, path, body=None, bearer=None, key=None):
        """A token server call through its proxy; `key`: the idempotency key it carries.
        (status, value) - or None for an answer that never came."""
        headers = {azlin_contract.CLIENT_HEADER: HELLO}
        if key:
            headers[azlin_contract.IDEMPOTENCY_HEADER] = key
        try:
            status, value, _ = self.client.call(method, path, body, bearer=bearer,
                                                headers=headers)
        except LOST:
            return None
        return status, value

    def signup(self, name):
        status, bundle, text = self.client.call('POST', '/v1/drives', {'name': name},
                                                headers={azlin_contract.CLIENT_HEADER: HELLO})
        expect(status == 201 and isinstance(bundle, dict),
               'no drive to test with: HTTP %d %s' % (status, text[:200]))
        return bundle

    def paid_drive(self, months=3):
        """A drive bought with the development checkout and the approving card: (checkout id,
        its sealed sign-up opened)."""
        secret, claim_key = azlin_claim.new_claim_key()
        order = {'tier': '100GB', 'months': months, 'method': 'card', 'claim_key': claim_key}
        status, value = self.call('POST', '/v1/checkout', order)
        checkout = (value or {}).get('checkout_id') or ''
        expect(status == 201 and checkout, 'no checkout: HTTP %d %r' % (status, code_of(value)))
        self.call('POST', '/v1/checkout/%s/pay' % checkout, {'card_number': APPROVING_CARD})
        answer = self.poll(checkout, lambda a: a.get('status') != 'pending', 60)
        expect(answer.get('status') == 'approved' and answer.get('sealed_signup'),
               'the paid checkout is %r' % answer.get('status'))
        return checkout, json.loads(azlin_claim.open_sealed(answer['sealed_signup'], secret,
                                                             checkout))

    def poll(self, checkout, done, seconds):
        deadline = time.time() + seconds
        answer = {}
        while time.time() < deadline:
            got = self.call('GET', '/v1/checkout/' + checkout)
            answer = (got[1] if got else None) or {}
            if done(answer):
                break
            time.sleep(0.5)
        return answer


def lost(world, rule, call):
    """Runs `call` with `rule` on the token proxy for it: the call's answer must be lost."""
    fault = world.token_proxy.add(**rule)
    got = call()
    world.token_proxy.clear()
    expect(fault.applied == 1, 'the fault met no request: %s' % fault.describe())
    if rule.get('action') == 'reset_after':
        expect(got is None, 'the answer was not lost: %r' % (got,))
    return got


# ---- the scenarios ----

SCENARIOS = []


def scenario(name, fixed_by):
    def register(fn):
        SCENARIOS.append((name, fixed_by, fn))
        return fn
    return register


@scenario('lost-refresh', 'L2 (the drive token spent once: the lost-answer grace)')
def lost_refresh(world):
    seen = []
    for how, rule in (('the connection reset after the rotation',
                       dict(op='Refresh', action='reset_after', times=1)),
                      ('503 heard after the rotation',
                       dict(op='Refresh', action='answer_after', status=503, code='unavailable',
                            message='the account database is not reachable; try again',
                            times=1))):
        bundle = world.signup('chaos-token-refresh')
        drive, first = bundle['drive']['id'], bundle['drive_token']
        path = '/v1/drives/%s/credentials' % drive
        lost(world, rule, lambda: world.call('POST', path, {}, bearer=first))
        again = world.call('POST', path, {}, bearer=first)
        expect(again is not None and again[0] == 200, '%s: the same token again is %s %r' % (
            how, again and again[0], again and code_of(again[1])))
        second = again[1].get('drive_token') or ''
        expect(family_of(second) == family_of(first), '%s: the answer is another family' % how)
        status, value = world.call('POST', path, {}, bearer=second)
        expect(status == 200, '%s: the new token does not refresh: %s %r' % (
            how, status, code_of(value)))
        status, value = world.call('POST', path, {}, bearer=first)
        expect(status == 401 and code_of(value) == 'token_reuse',
               '%s: the first token once more after the new one was used is %s %r, not a '
               'reuse' % (how, status, code_of(value)))
        seen.append(how)
    return 'answered after: %s' % '; '.join(seen)


@scenario('lost-lockdown', 'L4 (idempotent grants: the lockdown)')
def lost_lockdown(world):
    bundle = world.signup('chaos-token-lockdown')
    drive, owner = bundle['drive']['id'], bundle['drive_token']
    path = '/v1/drives/%s/lockdown' % drive
    key = idempotency_key()
    lost(world, dict(op='Lockdown', action='reset_after', times=1),
         lambda: world.call('POST', path, {}, bearer=owner, key=key))
    again = world.call('POST', path, {}, bearer=owner, key=key)
    expect(again is not None and again[0] == 200 and again[1].get('drive_token'),
           'the same device asking again is %s %r' % (again and again[0],
                                                     again and code_of(again[1])))
    status, value = world.call('POST', '/v1/drives/%s/credentials' % drive, {},
                               bearer=again[1]['drive_token'])
    expect(status == 200, "the grant's token does not refresh: %s %r" % (status, code_of(value)))
    return 'the repeated lockdown granted a working family'


@scenario('lost-claim', 'L4 (idempotent grants: the claim)')
def lost_claim(world):
    _, bundle = world.paid_drive()
    drive, ticket = bundle['drive']['id'], (bundle.get('claim') or {}).get('ticket')
    expect(bool(ticket), 'the sealed sign-up carries no claim ticket')
    path = '/v1/drives/%s/claim' % drive
    key = idempotency_key()
    lost(world, dict(op='Claim', action='reset_after', times=1),
         lambda: world.call('POST', path, {'ticket': ticket}, key=key))
    lefts, token = [], None
    for _ in range(2):
        got = world.call('POST', path, {'ticket': ticket}, key=key)
        expect(got is not None and got[0] == 201, 'the repeated claim is %s %r' % (
            got and got[0], got and code_of(got[1])))
        lefts.append(got[1].get('claims_left'))
        token = got[1].get('drive_token')
    expect(lefts == [2, 2], 'one pick-up asked three times counted as %r claims left, not '
                            '[2, 2]' % lefts)
    status, value = world.call('POST', '/v1/drives/%s/credentials' % drive, {}, bearer=token)
    expect(status == 200, "the pick-up's token does not refresh: %s %r" % (status, code_of(value)))
    return 'one pick-up counted, its token refreshes'


@scenario('lost-redeem', 'L6 (period tokens: the idempotent redeem)')
def lost_redeem(world):
    checkout, bundle = world.paid_drive(months=3)
    drive, token = bundle['drive']['id'], bundle['drive_token']
    grant = bundle.get('period_tokens') or {}
    status, value = world.call('GET', '/v1/tokens/keys')
    key = next((k for k in (value or {}).get('keys') or [] if k.get('tier') == '100GB'), {})
    n, e = azlin_period.parse_public_key_pem(key.get('public_key_pem') or '')
    # one month at a time: the first is due at the approval
    blinded = [azlin_period.blind(n, e, '100GB', key['year'])]
    status, value = world.call('POST', '/v1/tokens/issue', {
        'checkout_id': checkout, 'blinded': [b for _, b in blinded],
        'issue_key': grant.get('issue_key'), 'key_id': key.get('key_id')}, key=idempotency_key())
    expect(status == 200, 'the period tokens were not issued: %s %r' % (status, code_of(value)))
    tokens = [azlin_period.finalize(n, e, state, sig) for (state, _), sig in
              zip(blinded, value['blind_signatures'])]

    def period():
        status, info = world.call('GET', '/v1/drives/' + drive, bearer=token)
        expect(status == 200, "the drive's status: %s %r" % (status, code_of(info)))
        return unix_of(info.get('period_until'))
    before = period()
    path = '/v1/drives/%s/redeem' % drive
    redeem_key = idempotency_key()
    lost(world, dict(op='Redeem', action='reset_after', times=1),
         lambda: world.call('POST', path, tokens[0], bearer=token, key=redeem_key))
    again = world.call('POST', path, tokens[0], bearer=token, key=redeem_key)
    expect(again is not None and again[0] == 200, 'the same token again is %s %r: the month is '
                                                   'lost' % (again and again[0],
                                                             again and code_of(again[1])))
    grew = period() - max(before or 0, int(time.time()))
    expect(MONTH - 2 * 86400 <= grew <= MONTH + 2 * 86400,
           'the period grew by %.1f days, not one month' % (grew / 86400.0))
    return 'answered again, the period one month longer'


@scenario('paid-abandon', 'L5 (the checkout lifecycle), if the server expired it')
def paid_abandon(world):
    operator = world.operator
    secret, claim_key = azlin_claim.new_claim_key()
    order = {'tier': '100GB', 'months': 1, 'withdrawal_consent': True, 'claim_key': claim_key,
             'provider': operator.card_provider, 'method': 'card', 'surface': 'fields',
             'vat_country': 'DE', 'country': 'DE'}
    operator.hold_webhooks(True)
    try:
        status, value = world.call('POST', '/v1/checkout', order)
        checkout = (value or {}).get('checkout_id') or ''
        client_secret = ((value or {}).get('surface') or {}).get('client_secret') or ''
        expect(status == 201 and checkout and client_secret,
               'no card checkout on the fields: %s %r' % (status, code_of(value)))
        taken = operator.confirm_card(client_secret) or {}
        expect(taken.get('result') == 'succeeded', 'the provider did not take the card: %r'
               % taken.get('code'))
        path = '/v1/checkout/%s/abandon' % checkout

        def still_open(got):
            return got is not None and (
                (got[0] == 200 and got[1].get('status') in ('pending', 'approved'))
                or (got[0] == 409 and code_of(got[1]) == 'checkout_paid'))
        first = world.call('POST', path, {}, key=idempotency_key())
        expect(still_open(first), 'the paid session abandoned is %s %r: the payment is taken '
                                  'and the drive never comes' % (
                                      first and first[0], first and (
                                          code_of(first[1]) or first[1].get('status'))))
        abandon_key = idempotency_key()
        lost(world, dict(op='Abandon', action='reset_after', times=1),
             lambda: world.call('POST', path, {}, key=abandon_key))
        again = world.call('POST', path, {}, key=abandon_key)
        expect(still_open(again), 'the lost abandon asked again is %s %r' % (
            again and again[0], again and (code_of(again[1]) or again[1].get('status'))))
    finally:
        operator.hold_webhooks(False)
    answer = world.poll(checkout, lambda a: a.get('status') not in ('pending', None),
                        WEBHOOK_WAIT)
    expect(answer.get('status') == 'approved' and answer.get('sealed_signup'),
           'the late webhook left the checkout %r' % answer.get('status'))
    bundle = json.loads(azlin_claim.open_sealed(answer['sealed_signup'], secret, checkout))
    expect(str((bundle.get('drive') or {}).get('id') or '').startswith('d_'),
           'the sealed sign-up opens to no drive')
    return 'abandoned after paying: %s; the late webhook approved it' % (
        first[1].get('status') or code_of(first[1]))


def head_bucket(bucket):
    status, headers, _ = bucket.request('HEAD')
    expect(status == 200, 'HeadBucket: HTTP %d' % status)
    return headers


def refused_for_quota(answer):
    status, headers, _ = answer
    return status == 403 and headers.get('x-azlin-error') == 'quota_exceeded'


@scenario('quota', 'L7 (the quota: parts reserved, the check in the state machine)')
def quota(world):
    bundle = world.signup('chaos-token-quota')
    drive = bundle['drive']['id']
    bucket = world.bucket(bundle)
    used = int(head_bucket(bucket).get('x-azlin-used-bytes') or 0)
    limit = used + 12 * MIB
    world.operator.set_quota(drive, bucket.bucket, limit)
    deadline = time.time() + QUOTA_WAIT
    while time.time() < deadline:
        if head_bucket(bucket).get('x-azlin-quota-bytes') == str(limit):
            break
        time.sleep(1)
    expect(head_bucket(bucket).get('x-azlin-quota-bytes') == str(limit),
           'the node did not learn the quota in %d s' % QUOTA_WAIT)
    status, _, body = bucket.request('POST', 'big.bin', query={'uploads': ''})
    expect(status == 200, 'CreateMultipartUpload: HTTP %d' % status)
    upload = body.decode('utf-8').split('<UploadId>', 1)[1].split('</UploadId>', 1)[0]

    def part(number):
        return bucket.request('PUT', 'big.bin', query={'partNumber': str(number),
                                                        'uploadId': upload},
                              body=os.urandom(5 * MIB))
    for number in (1, 2):
        status, _, body = part(number)
        expect(status == 200, 'part %d within the quota: HTTP %d %s' % (number, status,
                                                                       body[:200]))
    third = part(3)
    small = bucket.request('PUT', 'small.bin', body=os.urandom(3 * MIB))
    bucket.request('DELETE', 'big.bin', query={'uploadId': upload})
    after_abort = bucket.request('PUT', 'small.bin', body=os.urandom(3 * MIB))
    answers = []

    def put(key):
        answers.append(bucket.request('PUT', key, body=os.urandom(5 * MIB))[0])
    threads = [threading.Thread(target=put, args=(k,)) for k in ('a.bin', 'b.bin')]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(120)
    stored = int(head_bucket(bucket).get('x-azlin-used-bytes') or 0)
    problems = []
    if not refused_for_quota(third):
        problems.append('the part past the quota is HTTP %d, not 403 quota_exceeded' % third[0])
    if not refused_for_quota(small):
        problems.append('a PUT that fits only without the unfinished upload\'s parts is HTTP %d'
                        % small[0])
    if after_abort[0] != 200:
        problems.append('after the abort the PUT is HTTP %d: the parts were not freed'
                        % after_abort[0])
    if sorted(answers) != [200, 403]:
        problems.append('two PUTs at once that each fit alone answered %r, not one of each'
                        % sorted(answers))
    if stored > limit:
        problems.append('the bucket stores %d bytes over its quota' % (stored - limit))
    expect(not problems, '; '.join(problems))
    return 'parts counted, freed by the abort, one of two concurrent PUTs refused'


@scenario('lost-complete', 'L8 (S3 idempotency: the completion verified by HEAD)')
def lost_complete(world):
    bundle = world.signup('chaos-token-complete')
    bucket = world.bucket(bundle)
    status, _, body = bucket.request('POST', 'whole.bin', query={'uploads': ''})
    expect(status == 200, 'CreateMultipartUpload: HTTP %d' % status)
    upload = body.decode('utf-8').split('<UploadId>', 1)[1].split('</UploadId>', 1)[0]
    parts = [os.urandom(5 * MIB), os.urandom(MIB)]
    etags = []
    for number, data in enumerate(parts, 1):
        status, headers, _ = bucket.request('PUT', 'whole.bin', query={
            'partNumber': str(number), 'uploadId': upload}, body=data)
        expect(status == 200, 'part %d: HTTP %d' % (number, status))
        etags.append(headers.get('etag'))
    xml = ('<CompleteMultipartUpload>%s</CompleteMultipartUpload>' % ''.join(
        '<Part><PartNumber>%d</PartNumber><ETag>%s</ETag></Part>' % (n, e)
        for n, e in enumerate(etags, 1))).encode('utf-8')
    fault = world.s3_proxy.add(op='CompleteMultipartUpload', action='reset_after', times=1)
    try:
        answer = bucket.request('POST', 'whole.bin', query={'uploadId': upload}, body=xml)
    except LOST:
        answer = None
    world.s3_proxy.clear()
    expect(fault.applied == 1 and answer is None, 'the completion\'s answer was not lost')
    want = '"%s-%d"' % (hashlib.md5(b''.join(hashlib.md5(p).digest() for p in parts))
                        .hexdigest(), len(parts))
    head = bucket.head('whole.bin')
    expect(head is not None, 'the completed object is not there')
    expect(head.get('etag') == want, 'its ETag is %r, not the multipart ETag %s the client '
                                     'computes' % (head.get('etag'), want))
    retry = bucket.request('POST', 'whole.bin', query={'uploadId': upload}, body=xml)
    expect(retry[0] in (200, 404), 'the completion asked again is HTTP %d' % retry[0])
    expect((bucket.head('whole.bin') or {}).get('etag') == want, 'the retry changed the object')
    return 'the object is there with the multipart ETag; the retry answered %d' % retry[0]


# ---- the run ----


def run(world, only=None):
    failures = 0
    for name, fixed_by, fn in SCENARIOS:
        if only and name not in only:
            continue
        started = time.time()
        try:
            said = fn(world)
            print('PASS  %-14s %s (%.0f s)' % (name, said, time.time() - started), flush=True)
        except Failed as e:
            failures += 1
            print('FAIL  %-14s %s - passes with %s' % (name, e, fixed_by), flush=True)
        except Exception:  # noqa: BLE001 - one scenario's crash is its failure, the rest run
            failures += 1
            print('FAIL  %-14s crashed:\n%s' % (name, traceback.format_exc()), flush=True)
        finally:
            world.clear()
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--mock', action='store_true',
                        help='start scripts/azlin_mock_stack.py on free ports and check it')
    parser.add_argument('--token-url')
    parser.add_argument('--s3-url', help="the S3 (else the bundle's endpoint)")
    parser.add_argument('--sandbox-url', help='the payment sandbox (its switches and cards)')
    parser.add_argument('--azctl', help='azctl, to set a bucket\'s quota')
    parser.add_argument('--env', default='dev', help="azctl's environment (default dev)")
    parser.add_argument('--only', help='scenarios to run, comma separated')
    parser.add_argument('--list', action='store_true', help='name the scenarios and stop')
    args = parser.parse_args()
    if args.list:
        for name, fixed_by, _ in SCENARIOS:
            print('%-14s %s' % (name, fixed_by))
        return 0
    only = set(args.only.split(',')) if args.only else None
    unknown = (only or set()) - {name for name, _, _ in SCENARIOS}
    if unknown:
        parser.error('no scenario %s (--list)' % ', '.join(sorted(unknown)))
    stack = None
    if args.mock:
        import azlin_mock_stack  # noqa: PLC0415 - only for --mock
        root = tempfile.mkdtemp(prefix='azlin-chaos-token-')
        stack = azlin_mock_stack.start(root, providers=azlin_mock_stack.DEFAULT_PROVIDERS)
        world = World(stack.token_url, stack.s3_url,
                      MockOperator(stack.token.state, stack.token_url))
    else:
        world = World(azlin_client.token_url_from(args.token_url), args.s3_url,
                      StackOperator(args.azctl, args.env, args.sandbox_url))
    try:
        failures = run(world, only)
    finally:
        world.stop()
        if stack is not None:
            stack.stop()
    print('PASS' if failures == 0 else 'FAIL: %d scenario(s)' % failures, flush=True)
    return min(failures, 100)


if __name__ == '__main__':
    sys.exit(main())
