#!/usr/bin/env python3
"""The test scripts' one client of the Azlin stack: the token server and a drive's bucket.

- `TokenClient(url)`: `signup(name)` (POST /v1/drives) and `refresh(drive_id, token)` (POST
  /v1/drives/<id>/credentials with the Bearer drive token), each answering `(status, json)`.
- `Bucket(bundle, endpoint=None)`: the drive's bucket with the bundle's S3 credentials (SigV4,
  path style, the session token signed in), through AzDrive's test client
  (examples/azul-drive/scripts/s3_server.Client): `list(prefix)`, `keys(prefix)`, `get`, `put`,
  `delete`, `head`.
- `token_url_from(...)`: where the token server is - `--token-url`, else `$AZLIN_TOKEN_URL`, else
  the shared Azlin config's `endpoints` (`$AZLIN_CONFIG`, else ~/.azlin/config.json), else the
  local stack's default (http://127.0.0.1:8081: `azctl dev up --processes`).

Used by scripts/azmail_seed_azlin.py, scripts/azlin_token_conformance.py and the azlin phase of
scripts/azmail_e2e.py. Standard library only.
"""
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from xml.etree import ElementTree

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, os.path.join(REPO, 'examples', 'azul-drive', 'scripts'))

import s3_server  # noqa: E402

# The local stack of azul-apps (`azctl dev up --processes`): the token server and the S3
# balancer. Only the test scripts default to them; AzMail itself has no built-in token server.
LOCAL_TOKEN_URL = 'http://127.0.0.1:8081'
LOCAL_S3_URL = 'http://127.0.0.1:9000'
# The spellings of the token server's key in the shared config's `endpoints` (AzMail's
# azlin::endpoints_from_config reads the same ones).
CONFIG_TOKEN_KEYS = ('token', 'token_url', 'tokenUrl', 'token_server', 'tokenServer')
S3_NS = '{http://s3.amazonaws.com/doc/2006-03-01/}'


def config_endpoint(keys):
    """The first of `keys` in the shared Azlin config's `endpoints`, if there is one."""
    var = os.environ.get('AZLIN_CONFIG')
    if var is not None and (not var.strip() or var.strip().lower() == 'off'):
        return None
    path = var.strip() if var else os.path.join(os.path.expanduser('~'), '.azlin', 'config.json')
    try:
        with open(path, encoding='utf-8') as f:
            section = (json.load(f) or {}).get('endpoints') or {}
    except (OSError, ValueError, AttributeError):
        return None
    for key in keys:
        value = section.get(key) if isinstance(section, dict) else None
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def token_url_from(flag=None):
    """`flag`, else $AZLIN_TOKEN_URL, else the shared config, else the local stack's."""
    for value in (flag, os.environ.get('AZLIN_TOKEN_URL'), config_endpoint(CONFIG_TOKEN_KEYS)):
        if value and value.strip():
            return value.strip().rstrip('/')
    return LOCAL_TOKEN_URL


class TokenClient:
    def __init__(self, url, timeout=20):
        self.url = url.rstrip('/')
        self.timeout = timeout

    def call(self, method, path, body=None, bearer=None):
        """`(status, parsed JSON or None, raw text)`; never raises for an HTTP error status."""
        data = None if body is None else json.dumps(body).encode('utf-8')
        request = urllib.request.Request(self.url + path, data=data, method=method)
        request.add_header('Accept', 'application/json')
        if data is not None:
            request.add_header('Content-Type', 'application/json')
        if bearer:
            request.add_header('Authorization', 'Bearer ' + bearer)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                status, raw = response.status, response.read()
        except urllib.error.HTTPError as e:
            status, raw = e.code, e.read()
        text = raw.decode('utf-8', 'replace')
        try:
            value = json.loads(text) if text.strip() else None
        except ValueError:
            value = None
        return status, value, text

    def health(self):
        return self.call('GET', '/health')[0]

    def signup(self, name='AzMail test', tier=None):
        body = {'name': name}
        if tier:
            body['tier'] = tier
        return self.call('POST', '/v1/drives', body)

    def refresh(self, drive_id, token):
        return self.call('POST', '/v1/drives/%s/credentials' % urllib.parse.quote(drive_id, safe=''),
                         {}, bearer=token)


def bundle_drive(bundle):
    """(drive id, bucket, endpoint, region) of a drive bundle."""
    drive = bundle.get('drive') or {}
    location = drive.get('location') or {}
    return (drive.get('id') or '', location.get('bucket') or drive.get('bucket') or '',
            location.get('endpoint') or (bundle.get('credentials') or {}).get('endpoint') or '',
            location.get('region') or s3_server.DEFAULT_REGION)


class Bucket:
    """A drive's bucket with its bundle's credentials."""

    def __init__(self, bundle, endpoint=None):
        _, self.bucket, bundle_endpoint, region = bundle_drive(bundle)
        credentials = bundle.get('credentials') or {}
        self.endpoint = (endpoint or bundle_endpoint).rstrip('/')
        self.session_token = credentials.get('session_token') or ''
        self.client = s3_server.Client(self.endpoint, credentials.get('access_key_id', ''),
                                       credentials.get('secret_access_key', ''), region=region)

    def request(self, method, key='', query=None, body=b'', headers=None):
        send = dict(headers or {})
        if self.session_token:
            send['x-amz-security-token'] = self.session_token
        return self.client.request(method, self.bucket, key, query=query, body=body, headers=send)

    def must(self, method, key='', query=None, body=b'', headers=None, ok=(200, 204, 206)):
        status, response_headers, data = self.request(method, key, query, body, headers)
        if status not in ok:
            raise RuntimeError('%s %s/%s: HTTP %d %s' % (method, self.bucket, key, status,
                                                        data[:300].decode('utf-8', 'replace')))
        return response_headers, data

    def list(self, prefix='', delimiter=None):
        """Every (key, size) and every common prefix under `prefix`, across pages."""
        objects, folders, token = [], [], None
        while True:
            query = {'list-type': '2', 'prefix': prefix, 'max-keys': '1000'}
            if delimiter:
                query['delimiter'] = delimiter
            if token:
                query['continuation-token'] = token
            _, data = self.must('GET', '', query=query)
            root = ElementTree.fromstring(data)
            for item in root.iter(S3_NS + 'Contents'):
                objects.append((item.findtext(S3_NS + 'Key'), int(item.findtext(S3_NS + 'Size') or 0)))
            for item in root.iter(S3_NS + 'CommonPrefixes'):
                folders.append(item.findtext(S3_NS + 'Prefix'))
            if (root.findtext(S3_NS + 'IsTruncated') or 'false') != 'true':
                return objects, folders
            token = root.findtext(S3_NS + 'NextContinuationToken')

    def keys(self, prefix=''):
        return [key for key, _ in self.list(prefix)[0]]

    def get(self, key):
        return self.must('GET', key)[1]

    def head(self, key):
        """The object's headers, or None when it is not there."""
        status, headers, _ = self.request('HEAD', key)
        return headers if status == 200 else None

    def put(self, key, data, content_type='message/rfc822'):
        self.must('PUT', key, body=data, headers={'Content-Type': content_type})

    def delete(self, key):
        self.must('DELETE', key, ok=(200, 204, 404))

    def presigned_get(self, key, expires=3600):
        """A public link of `key`: a presigned GET with the bundle's credentials."""
        return self.client.presign('GET', self.bucket, key, expires, self.session_token or None)

    def fetch(self, url):
        """A link fetched without credentials: (status, lowercase headers, body)."""
        return self.client.fetch(url)


# ---- the mailbox's names (examples/azul-mail/AZLIN_MAIL.md, azlin.rs) ----

STAMP = re.compile(r'^\d{8}T\d{6}Z-[0-9a-f]{16}$')


def object_name(data, stamp_secs):
    """`<YYYYMMDDTHHMMSSZ>-<first 16 hex of SHA-256>.eml`: what AzMail and the Worker name a
    message."""
    return '%s-%s.eml' % (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime(stamp_secs)),
                          hashlib.sha256(data).hexdigest()[:16])


def message_key(folder, name):
    return 'mail/%s/%s' % (folder, name)


def marker_key(message_id, flag):
    return 'mail/.state/%s/%s' % (message_id, flag)
