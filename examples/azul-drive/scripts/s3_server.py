#!/usr/bin/env python3
"""A local S3 for AzDrive's tests and for trying AzDrive without a cloud account.

Two backends:

- ``stdlib`` (the default, no pip): a small S3 subset server that keeps the objects in a
  local folder (``<root>/<bucket>/<key>``) and implements exactly what AzDrive and AzMail use:
  ListObjectsV2 (prefix, delimiter, max-keys, continuation-token, start-after), GetObject (one
  ``Range``), PutObject, CopyObject (``x-amz-copy-source``), DeleteObject, HeadObject, plus
  CreateBucket / HeadBucket / ListBuckets, and multipart uploads: CreateMultipartUpload,
  UploadPart, ListParts, CompleteMultipartUpload (conditional too) and AbortMultipartUpload
  (the uploads under way live in ``<root>/.s3-server-uploads``, so several servers over one
  root - the nodes of an E2E - share them). Conditional requests as S3 answers them: a PUT with
  ``If-None-Match: *`` writes only a new key, one with ``If-Match`` only over that version
  (412 PreconditionFailed otherwise); a GET / HEAD with ``If-None-Match`` of the current
  version answers 304 (a sync's compare-and-swap of its index, and its polling).
  Every request must be signed with AWS SigV4 (header-based) with the configured key; errors are
  S3's XML error bodies. Path-style (``/<bucket>/<key>``) and virtual-host style
  (``Host: <bucket>.<host>``) both work. Every request is logged, so a test can assert which
  objects were fetched (``Server.requests()``, ``Server.object_gets()``, ``--log file.jsonl``).
  The kill switches of a transfer E2E: after N parts (``hold_parts_after``,
  ``--hold-parts-after``) or N ranged GETs (``hold_gets_after``, ``--hold-gets-after``) the next
  ones wait until ``release()`` - so the client, or this node, can be killed half way.
- ``moto``: moto's ``ThreadedMotoServer`` when the ``moto`` package is installed (a much bigger
  S3; it does not check signatures and keeps no request log here).

Usage::

    python3 examples/azul-drive/scripts/s3_server.py --root /tmp/s3 --bucket azdrive
        [--host 127.0.0.1] [--port 9000] [--access-key azdrive-test]
        [--secret-key azdrive-test-secret] [--region us-east-1]
        [--backend stdlib|moto|auto] [--log requests.jsonl]
        [--hold-parts-after N] [--hold-gets-after N]

Then add a drive in AzDrive with endpoint ``http://127.0.0.1:9000``, region ``us-east-1``, the
bucket, the keys, and path-style URLs.

As a module (the tests, browse.py)::

    server = s3_server.start(root, access_key=..., secret_key=..., buckets=["azdrive"])
    server.url, server.requests(), server.object_gets(), server.stop()
    s3_server.Client(server.url, access_key, secret_key).request("GET", "azdrive", "a.txt")
"""

import argparse
import base64
import calendar
import email.utils
import hashlib
import hmac
import http.client
import http.server
import json
import mimetypes
import os
import shutil
import sys
import threading
import time
import urllib.parse
import uuid
import xml.etree.ElementTree as ElementTree
from collections import namedtuple
from xml.sax.saxutils import escape

EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
UNSIGNED_PAYLOAD = "UNSIGNED-PAYLOAD"
ALGORITHM = "AWS4-HMAC-SHA256"
S3_NS = "http://s3.amazonaws.com/doc/2006-03-01/"
MAX_SKEW_SECONDS = 15 * 60
MAX_KEYS = 1000
DEFAULT_ACCESS_KEY = "azdrive-test"
DEFAULT_SECRET_KEY = "azdrive-test-secret"
DEFAULT_REGION = "us-east-1"

# ---------------------------------------------------------------------------------------------
# SigV4 (the server checks with it, the test client signs with it)
# ---------------------------------------------------------------------------------------------

Signed = namedtuple("Signed", "canonical_request string_to_sign signed_headers signature")


def sha256_hex(data):
    return hashlib.sha256(data).hexdigest()


def uri_encode(text, encode_slash=True):
    """SigV4 URI encoding: A-Z a-z 0-9 - _ . ~ stay, / stays unless encode_slash."""
    out = []
    for byte in text.encode("utf-8"):
        c = chr(byte)
        if c.isascii() and (c.isalnum() or c in "-_.~") or (c == "/" and not encode_slash):
            out.append(c)
        else:
            out.append("%%%02X" % byte)
    return "".join(out)


def canonical_query(pairs):
    encoded = sorted((uri_encode(k), uri_encode(v)) for k, v in pairs)
    return "&".join("%s=%s" % kv for kv in encoded)


def _hmac(key, text):
    return hmac.new(key, text.encode("utf-8"), hashlib.sha256).digest()


def sign(secret, amz_date, region, service, method, canonical_uri, query_pairs, headers, payload_hash):
    """Signs one request. `headers` maps LOWERCASE names to raw values (all of them are signed)."""
    names = sorted(headers)
    header_block = "".join("%s:%s\n" % (n, " ".join(str(headers[n]).split())) for n in names)
    signed_headers = ";".join(names)
    canonical_request = "\n".join(
        [method, canonical_uri, canonical_query(query_pairs), header_block, signed_headers, payload_hash]
    )
    date = amz_date[:8]
    scope = "%s/%s/%s/aws4_request" % (date, region, service)
    string_to_sign = "\n".join(
        [ALGORITHM, amz_date, scope, sha256_hex(canonical_request.encode("utf-8"))]
    )
    key = _hmac(("AWS4" + secret).encode("utf-8"), date)
    key = _hmac(key, region)
    key = _hmac(key, service)
    key = _hmac(key, "aws4_request")
    signature = hmac.new(key, string_to_sign.encode("utf-8"), hashlib.sha256).hexdigest()
    return Signed(canonical_request, string_to_sign, signed_headers, signature)


def amz_date_of(unix_seconds):
    return time.strftime("%Y%m%dT%H%M%SZ", time.gmtime(unix_seconds))


def parse_amz_date(text):
    try:
        return calendar.timegm(time.strptime(text, "%Y%m%dT%H%M%SZ"))
    except (TypeError, ValueError):
        return None


def parse_authorization(value):
    """`AWS4-HMAC-SHA256 Credential=AK/date/region/s3/aws4_request, SignedHeaders=a;b, Signature=..`"""
    if not value or not value.startswith(ALGORITHM + " "):
        return None
    fields = {}
    for part in value[len(ALGORITHM) + 1:].split(","):
        name, _, val = part.strip().partition("=")
        fields[name] = val
    try:
        access_key, date, region, service, terminator = fields["Credential"].split("/")
        return {
            "access_key": access_key,
            "date": date,
            "region": region,
            "service": service,
            "terminator": terminator,
            "signed_headers": fields["SignedHeaders"].split(";"),
            "signature": fields["Signature"],
        }
    except (KeyError, ValueError):
        return None


# ---------------------------------------------------------------------------------------------
# The object store: <root>/<bucket>/<key>
# ---------------------------------------------------------------------------------------------


class BadKey(Exception):
    pass


def check_key(key):
    """Keys map to files, so this server refuses what cannot be one safely."""
    if not key:
        raise BadKey("the key is empty")
    if len(key.encode("utf-8")) > 1024:
        raise BadKey("the key is longer than 1024 bytes")
    if "\0" in key or "\\" in key:
        raise BadKey("this test server refuses keys with NUL or backslash")
    if key.endswith("/"):
        raise BadKey("this test server stores no folder marker objects (keys ending in /)")
    for segment in key.split("/"):
        if segment in ("", ".", ".."):
            raise BadKey("this test server refuses keys with empty, '.' or '..' segments")


class Store:
    TMP = ".s3-server-tmp"
    UPLOADS = ".s3-server-uploads"

    def __init__(self, root):
        self.root = os.path.abspath(root)
        os.makedirs(os.path.join(self.root, self.TMP), exist_ok=True)
        # A conditional write checks and writes under it: of two writers that read the same
        # version, exactly one wins.
        self.write_lock = threading.Lock()

    def bucket_dir(self, bucket):
        return os.path.join(self.root, bucket)

    def has_bucket(self, bucket):
        return (
            bool(bucket)
            and not bucket.startswith(".")
            and "/" not in bucket
            and os.path.isdir(self.bucket_dir(bucket))
        )

    def create_bucket(self, bucket):
        os.makedirs(self.bucket_dir(bucket), exist_ok=True)

    def buckets(self):
        return sorted(
            name for name in os.listdir(self.root)
            if not name.startswith(".") and os.path.isdir(os.path.join(self.root, name))
        )

    def path(self, bucket, key):
        check_key(key)
        path = os.path.join(self.bucket_dir(bucket), *key.split("/"))
        # Belt and braces: whatever check_key missed, the file stays in the bucket.
        if os.path.commonpath([path, self.bucket_dir(bucket)]) != self.bucket_dir(bucket):
            raise BadKey("the key leaves the bucket")
        return path

    def keys(self, bucket):
        """Every key of the bucket, sorted (UTF-8 byte order is code point order)."""
        base = self.bucket_dir(bucket)
        out = []
        for folder, dirs, files in os.walk(base):
            dirs[:] = [d for d in dirs if not d.startswith(".s3-server")]
            rel = os.path.relpath(folder, base)
            for name in files:
                key = name if rel == "." else "/".join(rel.split(os.sep) + [name])
                out.append(key)
        out.sort()
        return out

    def info(self, bucket, key):
        path = self.path(bucket, key)
        if not os.path.isfile(path):
            return None
        stat = os.stat(path)
        return {"size": stat.st_size, "mtime": stat.st_mtime, "etag": self.etag(path)}

    @staticmethod
    def etag(path):
        md5 = hashlib.md5()
        with open(path, "rb") as f:
            for block in iter(lambda: f.read(1 << 20), b""):
                md5.update(block)
        return '"%s"' % md5.hexdigest()

    def read(self, bucket, key, start=0, end=None):
        with open(self.path(bucket, key), "rb") as f:
            f.seek(start)
            return f.read() if end is None else f.read(end - start + 1)

    def write(self, bucket, key, data):
        path = self.path(bucket, key)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = os.path.join(self.root, self.TMP, uuid.uuid4().hex)
        with open(tmp, "wb") as f:
            f.write(data)
        os.replace(tmp, path)
        return self.etag(path)

    # -- multipart uploads ------------------------------------------------------------------

    def start_upload(self, bucket, key):
        """A new multipart upload of `key`: its id."""
        check_key(key)
        upload_id = uuid.uuid4().hex
        folder = os.path.join(self.root, self.UPLOADS, upload_id)
        os.makedirs(folder)
        with open(os.path.join(folder, "meta.json"), "w", encoding="utf-8") as f:
            json.dump({"bucket": bucket, "key": key, "started": time.time()}, f)
        return upload_id

    def upload(self, upload_id, bucket, key):
        """The folder of the upload `upload_id` of `key`, or None when there is no such upload."""
        if not upload_id or any(c not in "0123456789abcdef" for c in upload_id):
            return None
        folder = os.path.join(self.root, self.UPLOADS, upload_id)
        try:
            with open(os.path.join(folder, "meta.json"), encoding="utf-8") as f:
                meta = json.load(f)
        except (OSError, ValueError):
            return None
        if meta.get("bucket") != bucket or meta.get("key") != key:
            return None
        return folder

    @staticmethod
    def part_path(folder, number):
        return os.path.join(folder, "part-%05d" % number)

    def write_part(self, folder, number, data):
        tmp = os.path.join(self.root, self.TMP, uuid.uuid4().hex)
        with open(tmp, "wb") as f:
            f.write(data)
        os.replace(tmp, self.part_path(folder, number))
        return '"%s"' % hashlib.md5(data).hexdigest()

    def parts(self, folder):
        """(number, etag, size) of every part sent, by number."""
        out = []
        for name in sorted(os.listdir(folder)):
            if name.startswith("part-"):
                path = os.path.join(folder, name)
                out.append((int(name[5:]), self.etag(path), os.path.getsize(path)))
        return out

    def complete(self, folder, bucket, key, listed):
        """Joins the parts `listed` ((number, etag) pairs) into `key`; the object's ETag
        (S3's: the MD5 of the parts' MD5s, a dash, their count). InvalidPart when one of them
        was not sent or is another version."""
        if not listed:
            raise S3Error(400, "MalformedXML", "The XML you provided was not well-formed")
        digests = []
        for number, etag in listed:
            path = self.part_path(folder, number)
            if not os.path.isfile(path) or self.etag(path).strip('"') != etag.strip().strip('"'):
                raise S3Error(400, "InvalidPart", "One or more of the specified parts could "
                              "not be found.", PartNumber=number)
            digests.append(bytes.fromhex(self.etag(path).strip('"')))
        target = self.path(bucket, key)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        tmp = os.path.join(self.root, self.TMP, uuid.uuid4().hex)
        with open(tmp, "wb") as out:
            for number, _ in listed:
                with open(self.part_path(folder, number), "rb") as part:
                    shutil.copyfileobj(part, out, 1 << 20)
        os.replace(tmp, target)
        shutil.rmtree(folder, ignore_errors=True)
        return '"%s-%d"' % (hashlib.md5(b"".join(digests)).hexdigest(), len(listed))

    def abort(self, folder):
        shutil.rmtree(folder, ignore_errors=True)

    def delete(self, bucket, key):
        path = self.path(bucket, key)
        try:
            os.remove(path)
        except FileNotFoundError:
            return
        # S3 has no folders: a folder with no objects left is gone.
        base = self.bucket_dir(bucket)
        folder = os.path.dirname(path)
        while folder != base and os.path.isdir(folder) and not os.listdir(folder):
            os.rmdir(folder)
            folder = os.path.dirname(folder)


def encode_token(entry, is_prefix):
    raw = json.dumps({"after": entry, "prefix": is_prefix}).encode("utf-8")
    return base64.urlsafe_b64encode(raw).decode("ascii")


def decode_token(token):
    try:
        data = json.loads(base64.urlsafe_b64decode(token.encode("ascii")))
        return data["after"], bool(data["prefix"])
    except Exception:  # noqa: BLE001 - any malformed token is the same error
        return None


def list_entries(keys, prefix, delimiter, after, after_is_prefix, start_after):
    """(kind, value) in S3 order: ("key", k) or ("prefix", p), after the continuation point."""
    entries = []
    last_prefix = None
    for key in keys:
        if not key.startswith(prefix):
            continue
        if start_after and key <= start_after:
            continue
        if after is not None:
            if key <= after:
                continue
            if after_is_prefix and key.startswith(after):
                continue
        rest = key[len(prefix):]
        if delimiter and delimiter in rest:
            common = prefix + rest[: rest.index(delimiter) + len(delimiter)]
            if common != last_prefix:
                entries.append(("prefix", common))
                last_prefix = common
        else:
            entries.append(("key", key))
    return entries


# ---------------------------------------------------------------------------------------------
# HTTP
# ---------------------------------------------------------------------------------------------


def xml_document(root_name, children_xml):
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n<%s xmlns="%s">%s</%s>'
        % (root_name, S3_NS, children_xml, root_name)
    ).encode("utf-8")


def element(name, value):
    return "<%s>%s</%s>" % (name, escape(str(value)), name)


def iso8601(unix_seconds):
    return time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(unix_seconds))


def same_etag(header, etag):
    """Whether a conditional header names `etag` (quotes and a weak prefix aside)."""
    def bare(tag):
        tag = tag.strip()
        if tag.startswith("W/"):
            tag = tag[2:]
        return tag.strip('"')
    return any(bare(part) == bare(etag) for part in header.split(","))


class S3Error(Exception):
    def __init__(self, status, code, message, **extra):
        super().__init__(message)
        self.status, self.code, self.message, self.extra = status, code, message, extra

    def body(self, resource, request_id):
        parts = [element("Code", self.code), element("Message", self.message)]
        for name, value in self.extra.items():
            parts.append(element(name, value))
        parts.append(element("Resource", resource))
        parts.append(element("RequestId", request_id))
        return ('<?xml version="1.0" encoding="UTF-8"?>\n<Error>%s</Error>' % "".join(parts)).encode("utf-8")


def parse_range(header, size):
    """(start, end) inclusive for one `bytes=` range, or raise S3Error 416."""
    unit, _, spec = header.partition("=")
    if unit.strip() != "bytes" or "," in spec:
        raise S3Error(416, "InvalidRange", "only one bytes= range is supported")
    first, _, last = spec.strip().partition("-")
    try:
        if first == "":
            length = int(last)
            if length <= 0 or size == 0:
                raise ValueError
            return max(0, size - length), size - 1
        start = int(first)
        end = int(last) if last else size - 1
    except ValueError:
        raise S3Error(416, "InvalidRange", "The requested range is not satisfiable") from None
    if start >= size or end < start:
        raise S3Error(416, "InvalidRange", "The requested range is not satisfiable",
                      ActualObjectSize=size)
    return start, min(end, size - 1)


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "azdrive-s3-test/1"

    def log_message(self, fmt, *args):  # noqa: D401 - quiet unless asked
        if self.server.verbose:
            sys.stderr.write("[s3_server] %s\n" % (fmt % args))

    # -- routing ------------------------------------------------------------------------------

    def split_request(self):
        """(bucket, key, raw_path, query_pairs) for path-style or virtual-host style."""
        raw_path, _, raw_query = self.path.partition("?")
        query_pairs = urllib.parse.parse_qsl(raw_query, keep_blank_values=True)
        path = urllib.parse.unquote(raw_path)
        host = (self.headers.get("Host") or "").split(":")[0].lower()
        virtual = None
        for bucket in self.server.store.buckets():
            if host.startswith(bucket.lower() + "."):
                virtual = bucket
                break
        if virtual is not None:
            return virtual, path.lstrip("/"), raw_path, query_pairs
        bucket, _, key = path.lstrip("/").partition("/")
        return bucket, key, raw_path, query_pairs

    def respond(self, status, body=b"", headers=None):
        self.send_response(status)
        for name, value in (headers or {}).items():
            self.send_header(name, str(value))
        if status != 204:
            if "Content-Length" not in (headers or {}):
                self.send_header("Content-Length", str(len(body)))
        self.send_header("x-amz-request-id", self.request_id)
        self.end_headers()
        if body and self.command != "HEAD" and status != 204:
            self.wfile.write(body)
        self.status = status

    def check_write_conditions(self, info):
        """A conditional PUT's If-None-Match: * (no object yet) and If-Match (that version)."""
        none_match = self.headers.get("If-None-Match")
        if none_match is not None and none_match.strip() == "*" and info is not None:
            raise S3Error(412, "PreconditionFailed",
                          "At least one of the pre-conditions you specified did not hold",
                          Condition="If-None-Match")
        match = self.headers.get("If-Match")
        if match is not None and (info is None or not same_etag(match, info["etag"])):
            raise S3Error(412, "PreconditionFailed",
                          "At least one of the pre-conditions you specified did not hold",
                          Condition="If-Match")

    def fail(self, error, resource, headers=None):
        body = b"" if self.command == "HEAD" else error.body(resource, self.request_id)
        self.respond(
            error.status,
            body,
            dict({"Content-Type": "application/xml", "Content-Length": len(body)},
                 **(headers or {})),
        )

    # -- authentication -----------------------------------------------------------------------

    def authenticate(self, raw_path, query_pairs, payload_hash):
        server = self.server
        auth = parse_authorization(self.headers.get("Authorization"))
        if auth is None:
            raise S3Error(403, "AccessDenied", "Access Denied (the request is not signed with SigV4)")
        if auth["access_key"] != server.access_key:
            raise S3Error(403, "InvalidAccessKeyId",
                          "The AWS Access Key Id you provided does not exist in our records.",
                          AWSAccessKeyId=auth["access_key"])
        if auth["region"] != server.region:
            raise S3Error(400, "AuthorizationHeaderMalformed",
                          "The authorization header is malformed; the region '%s' is wrong; "
                          "expecting '%s'" % (auth["region"], server.region),
                          Region=server.region)
        amz_date = self.headers.get("x-amz-date") or ""
        when = parse_amz_date(amz_date)
        if when is None:
            raise S3Error(403, "AccessDenied", "AWS authentication requires a valid x-amz-date header")
        if abs(time.time() - when) > MAX_SKEW_SECONDS:
            raise S3Error(403, "RequestTimeTooSkewed",
                          "The difference between the request time and the current time is too large.",
                          RequestTime=amz_date, ServerTime=amz_date_of(time.time()))
        if "host" not in auth["signed_headers"]:
            raise S3Error(400, "AuthorizationHeaderMalformed", "the Host header must be signed")
        headers = {}
        for name in auth["signed_headers"]:
            values = self.headers.get_all(name)
            if values is None:
                raise S3Error(403, "SignatureDoesNotMatch",
                              "the signed header %s is missing from the request" % name)
            headers[name] = ",".join(values)
        canonical_uri = uri_encode(urllib.parse.unquote(raw_path), encode_slash=False)
        signed = sign(server.secret_key, amz_date, auth["region"], auth["service"], self.command,
                      canonical_uri, query_pairs, headers, payload_hash)
        if not hmac.compare_digest(signed.signature, auth["signature"]):
            raise S3Error(403, "SignatureDoesNotMatch",
                          "The request signature we calculated does not match the signature you "
                          "provided. Check your key and signing method.",
                          StringToSign=signed.string_to_sign,
                          CanonicalRequest=signed.canonical_request)

    # -- verbs --------------------------------------------------------------------------------

    def handle_any(self):
        self.request_id = uuid.uuid4().hex[:16].upper()
        self.status = 0
        bucket, key, raw_path, query_pairs = self.split_request()
        query = dict(query_pairs)
        record = {
            "time": time.time(),
            "method": self.command,
            "bucket": bucket,
            "key": key,
            "query": query,
            "range": self.headers.get("Range"),
            "op": None,
        }
        resource = "/" + "/".join(p for p in (bucket, key) if p)
        body = b""
        try:
            length = self.headers.get("Content-Length")
            if length:
                body = self.rfile.read(int(length))
            elif self.command == "PUT" and self.headers.get("Transfer-Encoding"):
                raise S3Error(411, "MissingContentLength", "this test server needs Content-Length")
            payload_hash = self.headers.get("x-amz-content-sha256")
            if self.headers.get("Authorization") and not payload_hash:
                raise S3Error(400, "InvalidRequest", "Missing required header for this request: "
                                                     "x-amz-content-sha256")
            self.authenticate(raw_path, query_pairs, payload_hash or EMPTY_SHA256)
            fault = self.server.fault_of(bucket)
            if fault is not None:
                status, code, message, headers = fault
                record["op"] = "Fault"
                self.fail(S3Error(status, code, message), resource, headers)
                return
            if payload_hash and payload_hash.startswith("STREAMING-"):
                raise S3Error(501, "NotImplemented", "streaming (chunked) uploads are not supported")
            if payload_hash and payload_hash != UNSIGNED_PAYLOAD and payload_hash != sha256_hex(body):
                raise S3Error(400, "XAmzContentSHA256Mismatch",
                              "The provided 'x-amz-content-sha256' header does not match what was "
                              "computed.")
            record["op"] = self.dispatch(bucket, key, query, body)
        except BadKey as e:
            record["op"] = record["op"] or "Refused"
            self.fail(S3Error(400, "InvalidArgument", str(e)), resource)
        except S3Error as e:
            self.fail(e, resource)
        except (ConnectionError, BrokenPipeError):
            self.status = 499
        finally:
            record["status"] = self.status
            record["request_id"] = self.request_id
            self.server.record(record)

    def dispatch(self, bucket, key, query, body):
        store = self.server.store
        if not bucket:
            if self.command == "GET":
                items = "".join(
                    "<Bucket>%s%s</Bucket>" % (element("Name", b), element("CreationDate", iso8601(0)))
                    for b in store.buckets()
                )
                self.respond(200, xml_document("ListAllMyBucketsResult", "<Buckets>%s</Buckets>" % items),
                             {"Content-Type": "application/xml"})
                return "ListBuckets"
            raise S3Error(405, "MethodNotAllowed", "The specified method is not allowed.")
        if not key and self.command == "PUT":
            store.create_bucket(bucket)
            self.respond(200, headers={"Location": "/" + bucket})
            return "CreateBucket"
        if not store.has_bucket(bucket):
            raise S3Error(404, "NoSuchBucket", "The specified bucket does not exist", BucketName=bucket)
        if not key:
            if self.command == "HEAD":
                self.respond(200)
                return "HeadBucket"
            if self.command == "GET":
                if query.get("list-type") != "2":
                    raise S3Error(501, "NotImplemented", "this test server lists with ListObjectsV2 "
                                                         "(list-type=2) only")
                self.list_objects(bucket, query)
                return "ListObjectsV2"
            raise S3Error(405, "MethodNotAllowed", "The specified method is not allowed.")
        if self.command == "POST" and "uploads" in query:
            upload_id = store.start_upload(bucket, key)
            self.respond(200, xml_document("InitiateMultipartUploadResult", element("Bucket", bucket)
                                           + element("Key", key) + element("UploadId", upload_id)),
                         {"Content-Type": "application/xml"})
            return "CreateMultipartUpload"
        if "uploadId" in query:
            return self.multipart(bucket, key, query, body)
        if self.command in ("GET", "HEAD"):
            if self.command == "GET" and self.headers.get("Range"):
                self.server.gate("gets")
            info = store.info(bucket, key)
            if info is None:
                raise S3Error(404, "NoSuchKey", "The specified key does not exist.", Key=key)
            none_match = self.headers.get("If-None-Match")
            if none_match is not None and same_etag(none_match, info["etag"]):
                self.respond(304, headers={"ETag": info["etag"]})
                return "HeadObject" if self.command == "HEAD" else "GetObject"
            headers = {
                "Content-Type": mimetypes.guess_type(key)[0] or "application/octet-stream",
                "ETag": info["etag"],
                "Last-Modified": email.utils.formatdate(info["mtime"], usegmt=True),
                "Accept-Ranges": "bytes",
            }
            range_header = self.headers.get("Range")
            if range_header:
                start, end = parse_range(range_header, info["size"])
                headers["Content-Range"] = "bytes %d-%d/%d" % (start, end, info["size"])
                headers["Content-Length"] = end - start + 1
                data = b"" if self.command == "HEAD" else store.read(bucket, key, start, end)
                self.respond(206, data, headers)
            else:
                headers["Content-Length"] = info["size"]
                data = b"" if self.command == "HEAD" else store.read(bucket, key)
                self.respond(200, data, headers)
            return "HeadObject" if self.command == "HEAD" else "GetObject"
        if self.command == "PUT" and self.headers.get("x-amz-copy-source") is not None:
            source = urllib.parse.unquote(self.headers["x-amz-copy-source"].split("?", 1)[0])
            source_bucket, _, source_key = source.lstrip("/").partition("/")
            if not store.has_bucket(source_bucket):
                raise S3Error(404, "NoSuchBucket", "The specified bucket does not exist",
                              BucketName=source_bucket)
            if not source_key or store.info(source_bucket, source_key) is None:
                raise S3Error(404, "NoSuchKey", "The specified key does not exist.", Key=source_key)
            etag = store.write(bucket, key, store.read(source_bucket, source_key))
            result = element("LastModified", iso8601(store.info(bucket, key)["mtime"])) + element("ETag", etag)
            self.respond(200, xml_document("CopyObjectResult", result), {"Content-Type": "application/xml"})
            return "CopyObject"
        if self.command == "PUT":
            with store.write_lock:
                self.check_write_conditions(store.info(bucket, key))
                etag = store.write(bucket, key, body)
            self.respond(200, headers={"ETag": etag})
            return "PutObject"
        if self.command == "DELETE":
            store.delete(bucket, key)
            self.respond(204)
            return "DeleteObject"
        raise S3Error(405, "MethodNotAllowed", "The specified method is not allowed.")

    def multipart(self, bucket, key, query, body):
        """UploadPart, ListParts, CompleteMultipartUpload and AbortMultipartUpload."""
        store = self.server.store
        upload_id = query.get("uploadId", "")
        folder = store.upload(upload_id, bucket, key)
        if folder is None:
            raise S3Error(404, "NoSuchUpload", "The specified upload does not exist. The upload ID "
                          "may be invalid, or the upload may have been aborted or completed.",
                          UploadId=upload_id)
        if self.command == "PUT":
            try:
                number = int(query.get("partNumber", ""))
                if not 1 <= number <= 10000:
                    raise ValueError
            except ValueError:
                raise S3Error(400, "InvalidArgument", "Part number must be an integer between 1 "
                              "and 10000, inclusive") from None
            self.server.gate("parts")
            etag = store.write_part(folder, number, body)
            self.respond(200, headers={"ETag": etag})
            return "UploadPart"
        if self.command == "GET":
            parts = "".join(
                "<Part>%s%s%s</Part>" % (element("PartNumber", n), element("ETag", e), element("Size", size))
                for n, e, size in store.parts(folder)
            )
            self.respond(200, xml_document("ListPartsResult", element("Bucket", bucket)
                                           + element("Key", key) + element("UploadId", upload_id)
                                           + element("IsTruncated", "false") + parts),
                         {"Content-Type": "application/xml"})
            return "ListParts"
        if self.command == "DELETE":
            store.abort(folder)
            self.respond(204)
            return "AbortMultipartUpload"
        if self.command == "POST":
            try:
                root = ElementTree.fromstring(body)
            except ElementTree.ParseError:
                raise S3Error(400, "MalformedXML", "The XML you provided was not well-formed") from None
            listed = []
            for part in root.iter():
                if part.tag.rsplit("}", 1)[-1] != "Part":
                    continue
                fields = {child.tag.rsplit("}", 1)[-1]: (child.text or "") for child in part}
                try:
                    listed.append((int(fields.get("PartNumber", "")), fields.get("ETag", "")))
                except ValueError:
                    raise S3Error(400, "MalformedXML", "A part has no number") from None
            with store.write_lock:
                self.check_write_conditions(store.info(bucket, key))
                etag = store.complete(folder, bucket, key, listed)
            self.respond(200, xml_document("CompleteMultipartUploadResult", element("Bucket", bucket)
                                           + element("Key", key) + element("ETag", etag)),
                         {"Content-Type": "application/xml"})
            return "CompleteMultipartUpload"
        raise S3Error(405, "MethodNotAllowed", "The specified method is not allowed.")

    def list_objects(self, bucket, query):
        store = self.server.store
        prefix = query.get("prefix", "")
        delimiter = query.get("delimiter", "")
        start_after = query.get("start-after", "")
        encode = query.get("encoding-type") == "url"
        try:
            max_keys = min(int(query.get("max-keys", MAX_KEYS)), MAX_KEYS)
            if max_keys < 0:
                raise ValueError
        except ValueError:
            raise S3Error(400, "InvalidArgument", "max-keys must be a number from 0 to 1000") from None
        token = query.get("continuation-token")
        after, after_is_prefix = None, False
        if token is not None:
            decoded = decode_token(token)
            if decoded is None:
                raise S3Error(400, "InvalidArgument", "The continuation token provided is incorrect")
            after, after_is_prefix = decoded
        entries = list_entries(store.keys(bucket), prefix, delimiter, after, after_is_prefix, start_after)
        page, rest = entries[:max_keys], entries[max_keys:]

        def out(value):
            return urllib.parse.quote(value, safe="/") if encode else value

        parts = [
            element("Name", bucket),
            element("Prefix", out(prefix)),
            element("KeyCount", len(page)),
            element("MaxKeys", max_keys),
        ]
        if delimiter:
            parts.append(element("Delimiter", out(delimiter)))
        if encode:
            parts.append(element("EncodingType", "url"))
        parts.append(element("IsTruncated", "true" if rest else "false"))
        if token is not None:
            parts.append(element("ContinuationToken", token))
        if start_after:
            parts.append(element("StartAfter", out(start_after)))
        for kind, value in page:
            if kind == "key":
                info = store.info(bucket, value)
                parts.append(
                    "<Contents>%s%s%s%s%s</Contents>"
                    % (
                        element("Key", out(value)),
                        element("LastModified", iso8601(info["mtime"])),
                        element("ETag", info["etag"]),
                        element("Size", info["size"]),
                        element("StorageClass", "STANDARD"),
                    )
                )
            else:
                parts.append("<CommonPrefixes>%s</CommonPrefixes>" % element("Prefix", out(value)))
        if rest and page:
            kind, value = page[-1]
            parts.append(element("NextContinuationToken", encode_token(value, kind == "prefix")))
        self.respond(200, xml_document("ListBucketResult", "".join(parts)),
                     {"Content-Type": "application/xml"})

    do_GET = handle_any
    do_HEAD = handle_any
    do_PUT = handle_any
    do_DELETE = handle_any
    do_POST = handle_any


class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address, store, access_key, secret_key, region, log_path=None, verbose=False):
        super().__init__(address, Handler)
        self.store = store
        self.access_key = access_key
        self.secret_key = secret_key
        self.region = region
        self.verbose = verbose
        self.log_path = log_path
        self._log = []
        self._lock = threading.Lock()
        self._thread = None
        # Buckets answering an error to every request (an E2E's switch): bucket -> (status,
        # code, message, extra headers).
        self._faults = {}
        # The kill switches: kind ("parts", "gets") -> how many pass before the rest wait.
        self._holds = {}
        self._counts = {}
        self._waiting = 0
        self._released = threading.Event()
        self._stopping = False

    def hold_parts_after(self, count):
        """After `count` more UploadParts, every next one waits until `release`."""
        self._hold("parts", count)

    def hold_gets_after(self, count):
        """After `count` more ranged GetObjects, every next one waits until `release`."""
        self._hold("gets", count)

    def _hold(self, kind, count):
        with self._lock:
            self._holds[kind] = count
            self._counts[kind] = 0
            self._released.clear()

    def release(self):
        """Lets every held request go on, and holds no more."""
        with self._lock:
            self._holds.clear()
            self._released.set()

    def held(self):
        """How many requests wait right now."""
        with self._lock:
            return self._waiting

    def gate(self, kind):
        """Waits here while a request of `kind` is past its switch's count."""
        with self._lock:
            limit = self._holds.get(kind)
            if limit is None:
                return
            self._counts[kind] = self._counts.get(kind, 0) + 1
            if self._counts[kind] <= limit:
                return
            self._waiting += 1
        try:
            while not self._released.wait(0.2):
                if self._stopping:
                    break
        finally:
            with self._lock:
                self._waiting -= 1

    def fail_bucket(self, bucket, status, code, message, headers=None):
        """Every request to `bucket` answers this S3 error (with `headers`, e.g. an Azlin
        node's `x-azlin-error` and `Retry-After`) until `clear_faults`."""
        with self._lock:
            self._faults[bucket] = (status, code, message, dict(headers or {}))

    def clear_faults(self):
        with self._lock:
            self._faults.clear()

    def fault_of(self, bucket):
        with self._lock:
            return self._faults.get(bucket)

    @property
    def url(self):
        host, port = self.server_address[:2]
        return "http://%s:%d" % (host, port)

    def record(self, entry):
        with self._lock:
            self._log.append(entry)
            if self.log_path:
                with open(self.log_path, "a", encoding="utf-8") as f:
                    f.write(json.dumps(entry) + "\n")

    def requests(self):
        """Every request so far: method, op (ListObjectsV2, GetObject, ...), bucket, key, query,
        range, status."""
        with self._lock:
            return [dict(e) for e in self._log]

    def object_gets(self):
        """The keys of every GetObject so far (ranged ones included), in order."""
        return [e["key"] for e in self.requests() if e.get("op") == "GetObject"]

    def clear_log(self):
        with self._lock:
            self._log.clear()

    def start_background(self):
        self._thread = threading.Thread(target=self.serve_forever, name="s3_server", daemon=True)
        self._thread.start()
        return self

    def stop(self):
        self._stopping = True
        self._released.set()
        self.shutdown()
        self.server_close()
        if self._thread:
            self._thread.join(timeout=5)


def start(root, host="127.0.0.1", port=0, access_key=DEFAULT_ACCESS_KEY, secret_key=DEFAULT_SECRET_KEY,
          region=DEFAULT_REGION, buckets=(), log_path=None, verbose=False):
    """The stdlib server on `host:port` (0 = a free port), serving in a background thread."""
    store = Store(root)
    for bucket in buckets:
        store.create_bucket(bucket)
    server = Server((host, port), store, access_key, secret_key, region, log_path, verbose)
    return server.start_background()


class MotoServer:
    """moto's ThreadedMotoServer behind the same small interface (no request log)."""

    def __init__(self, host, port, access_key, secret_key, region, buckets):
        from moto.server import ThreadedMotoServer  # noqa: PLC0415 - optional dependency

        self._server = ThreadedMotoServer(ip_address=host, port=port or 9000)
        self._server.start()
        self.url = "http://%s:%d" % (host, port or 9000)
        client = Client(self.url, access_key, secret_key, region=region)
        for bucket in buckets:
            client.request("PUT", bucket, "")

    def requests(self):
        return None

    def object_gets(self):
        return None

    def stop(self):
        self._server.stop()


def moto_available():
    try:
        import moto.server  # noqa: F401, PLC0415

        return True
    except ImportError:
        return False


# ---------------------------------------------------------------------------------------------
# A small signing client (the tests; seeding moto)
# ---------------------------------------------------------------------------------------------


class Client:
    def __init__(self, url, access_key, secret_key, region=DEFAULT_REGION, path_style=True):
        parsed = urllib.parse.urlsplit(url)
        self.host, self.port = parsed.hostname, parsed.port or 80
        self.access_key, self.secret_key = access_key, secret_key
        self.region, self.path_style = region, path_style

    def request(self, method, bucket, key="", query=None, body=b"", headers=None, sign=True,
                now=None, payload_hash=None):
        """(status, lowercase headers, body)."""
        query_pairs = sorted((query or {}).items())
        if self.path_style:
            host = "%s:%d" % (self.host, self.port)
            path = "/" + bucket + ("/" + key if key else "")
        else:
            host = "%s.localhost:%d" % (bucket, self.port)
            path = "/" + key
        canonical_uri = uri_encode(path, encode_slash=False)
        send = dict(headers or {})
        if sign:
            amz_date = amz_date_of(time.time() if now is None else now)
            payload = payload_hash or sha256_hex(body)
            send["x-amz-date"] = amz_date
            send["x-amz-content-sha256"] = payload
            signed_headers = {k.lower(): v for k, v in send.items()}
            signed_headers["host"] = host
            signed = globals()["sign"](self.secret_key, amz_date, self.region, "s3", method,
                                       canonical_uri, query_pairs, signed_headers, payload)
            send["Authorization"] = "%s Credential=%s/%s/%s/s3/aws4_request, SignedHeaders=%s, Signature=%s" % (
                ALGORITHM, self.access_key, amz_date[:8], self.region, signed.signed_headers,
                signed.signature)
        target = canonical_uri + ("?" + canonical_query(query_pairs) if query_pairs else "")
        connection = http.client.HTTPConnection(self.host, self.port, timeout=10)
        try:
            connection.putrequest(method, target, skip_host=True, skip_accept_encoding=True)
            connection.putheader("Host", host)
            for name, value in send.items():
                connection.putheader(name, value)
            connection.putheader("Content-Length", str(len(body)))
            connection.endheaders(body if body else None)
            response = connection.getresponse()
            data = response.read()
            return response.status, {k.lower(): v for k, v in response.getheaders()}, data
        finally:
            connection.close()


# ---------------------------------------------------------------------------------------------
# Command line
# ---------------------------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", default=os.path.join(os.getcwd(), "s3-data"),
                        help="folder holding <bucket>/<key> (stdlib backend)")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=9000)
    parser.add_argument("--access-key", default=DEFAULT_ACCESS_KEY)
    parser.add_argument("--secret-key", default=DEFAULT_SECRET_KEY)
    parser.add_argument("--region", default=DEFAULT_REGION)
    parser.add_argument("--bucket", action="append", default=[], help="create this bucket (repeatable)")
    parser.add_argument("--backend", choices=["stdlib", "moto", "auto"], default="stdlib",
                        help="stdlib (default, no pip), moto (needs `pip install moto[server]`), "
                             "auto (moto when installed)")
    parser.add_argument("--log", help="append every request as a JSON line to this file")
    parser.add_argument("--verbose", action="store_true")
    parser.add_argument("--hold-parts-after", type=int, metavar="N",
                        help="after N UploadParts the next ones wait (an E2E's kill switch)")
    parser.add_argument("--hold-gets-after", type=int, metavar="N",
                        help="after N ranged GETs the next ones wait (an E2E's kill switch)")
    args = parser.parse_args(argv)

    backend = args.backend
    if backend == "auto":
        backend = "moto" if moto_available() else "stdlib"
    if backend == "moto":
        if not moto_available():
            parser.error("the moto backend needs `pip install 'moto[server]'`")
        server = MotoServer(args.host, args.port, args.access_key, args.secret_key, args.region,
                            args.bucket)
    else:
        server = start(args.root, args.host, args.port, args.access_key, args.secret_key,
                       args.region, args.bucket, args.log, args.verbose)
        if args.hold_parts_after is not None:
            server.hold_parts_after(args.hold_parts_after)
        if args.hold_gets_after is not None:
            server.hold_gets_after(args.hold_gets_after)
    print("S3_SERVER_URL %s" % server.url, flush=True)
    print("[s3_server] %s backend on %s, region %s, buckets %s%s" % (
        backend, server.url, args.region, ", ".join(args.bucket) or "(none created)",
        "" if backend == "moto" else ", data in " + os.path.abspath(args.root)), file=sys.stderr)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass
    finally:
        server.stop()


if __name__ == "__main__":
    main()
