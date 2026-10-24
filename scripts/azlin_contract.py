#!/usr/bin/env python3
"""What the Azlin apps and services say to each other, for the Python tools: read from the
contract crate's golden fixtures (examples/azlin-contract/fixtures, written by its tests), so
the fault proxy, the mock stack and the chaos scripts keep no copies that could drift.

- `azlin_errors()`: a node's codes (x-azlin-error) -> (HTTP status, S3 code, Retry-After or None)
- `contract_version()`, `client_hello(app, version)`: the `x-azlin-client` header's value
- `idempotency_key(raw=None)`: an `x-azlin-idempotency` value (base64url of 16 bytes, no padding)
- `is_idempotency_key(text)`: whether a header value is one; `idempotent_scopes()` the scopes a
  key counts in, `ALREADY_ANSWERED` the refusal of a repeat whose grant is in use

Standard library only.
"""
import base64
import json
import os
import re
import secrets

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.join(HERE, '..', 'examples', 'azlin-contract')
FIXTURES = os.path.join(CRATE, 'fixtures')
CLIENT_HEADER = 'x-azlin-client'
IDEMPOTENCY_HEADER = 'x-azlin-idempotency'

_cache = {}


def fixture(name):
    """One golden fixture's JSON."""
    if name not in _cache:
        with open(os.path.join(FIXTURES, name), encoding='utf-8') as f:
            _cache[name] = json.load(f)
    return _cache[name]


def azlin_errors():
    """{code: (status, S3 code, Retry-After seconds or None)} of every node code."""
    return {e['code']: (e['status'], e['s3'], e['retry_after'])
            for e in fixture('azlin_errors.json')}


def contract_version():
    """The crate's CONTRACT_VERSION (src/lib.rs), the number every hello carries."""
    if 'version' not in _cache:
        with open(os.path.join(CRATE, 'src', 'lib.rs'), encoding='utf-8') as f:
            found = re.search(r'^pub const CONTRACT_VERSION: u32 = (\d+);$', f.read(), re.M)
        if not found:
            raise ValueError('no CONTRACT_VERSION in the contract crate')
        _cache['version'] = int(found.group(1))
    return _cache['version']


def client_hello(app, version):
    """The hello of `app` at `version`: `azdrive/0.3.0; contract=3`."""
    return '%s/%s; contract=%d' % (app, version, contract_version())


def idempotency_key(raw=None):
    """An idempotency key's header value: `raw` (16 bytes; else fresh random ones) as
    base64url without padding."""
    raw = secrets.token_bytes(16) if raw is None else bytes(raw)
    if len(raw) != 16:
        raise ValueError('an idempotency key is 16 bytes')
    return base64.urlsafe_b64encode(raw).decode('ascii').rstrip('=')


def is_idempotency_key(text):
    """Whether `text` reads as an idempotency key (22 characters of base64url whose last 4 bits
    are zero, as the crate's IdempotencyKey::parse takes them)."""
    text = (text or '').strip()
    if len(text) != 22 or not re.fullmatch(r'[A-Za-z0-9_-]{22}', text):
        return False
    raw = base64.urlsafe_b64decode(text + '==')
    return len(raw) == 16 and idempotency_key(raw) == text


def idempotent_scopes():
    """The scopes an idempotency key counts in (`lockdown`, `claim`, ...)."""
    return list(fixture('idempotent_scopes.json')['scopes'])


ALREADY_ANSWERED = 'already_answered'
# idempotency.rs IDEMPOTENCY_REQUIRED: an app's grant sent without its key.
IDEMPOTENCY_REQUIRED = 'idempotency_required'


def min_contract():
    """The crate's MIN_CONTRACT (src/hello.rs): the oldest contract the services answer."""
    if 'min' not in _cache:
        with open(os.path.join(CRATE, 'src', 'hello.rs'), encoding='utf-8') as f:
            found = re.search(r'^pub const MIN_CONTRACT: u32 = (\d+);$', f.read(), re.M)
        if not found:
            raise ValueError('no MIN_CONTRACT in the contract crate')
        _cache['min'] = int(found.group(1))
    return _cache['min']


def below_minimum(hello):
    """Whether the hello `hello` (`azdrive/0.3.0; contract=3`) is refused 426 client_too_old
    (hello.rs `ClientMinimum::refuses`): one that does not read, or an older contract. The
    per-app version floors (MIN_VERSIONS) are empty."""
    first, _, rest = hello.partition(';')
    app, _, version = first.strip().partition('/')
    if not re.fullmatch(r'[a-z0-9-]{1,32}', app.strip().lower()) \
            or not re.fullmatch(r'\d+\.\d+\.\d+([-+].*)?', version.strip()):
        return True
    contract = None
    for field in rest.split(';'):
        key, _, value = field.partition('=')
        if key.strip() == 'contract' and value.strip().isdigit():
            contract = int(value.strip())
            break
    return contract is None or contract < min_contract()
