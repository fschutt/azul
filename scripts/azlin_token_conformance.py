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

Every drive token is secret: none is printed.
"""
import argparse
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import azlin_client  # noqa: E402

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
    return suite.failures


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
