#!/usr/bin/env python3
"""Unit tests of s3_server.py, the local S3 for AzDrive's tests (stdlib only).

    python3 -m unittest discover -s examples/azul-drive/scripts -p 'test_*.py'
    python3 examples/azul-drive/scripts/test_s3_server.py
"""

import os
import shutil
import sys
import tempfile
import time
import unittest
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import s3_server  # noqa: E402

ACCESS = "AKIDTEST"
SECRET = "test-secret"
BUCKET = "azdrive"

SUITE_SECRET = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY"
S3_SECRET = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"


def local_name(tag):
    return tag.rsplit("}", 1)[-1]


def xml_values(body, name):
    """Every text of the elements called `name`, in document order."""
    root = ET.fromstring(body)
    return [e.text or "" for e in root.iter() if local_name(e.tag) == name]


def xml_code(body):
    codes = xml_values(body, "Code")
    return codes[0] if codes else None


class SigV4Vectors(unittest.TestCase):
    """The server checks signatures with the same code the vectors pin."""

    def test_get_vanilla_matches_the_aws_suite(self):
        signed = s3_server.sign(
            SUITE_SECRET,
            "20150830T123600Z",
            "us-east-1",
            "service",
            "GET",
            "/",
            [],
            {"host": "example.amazonaws.com", "x-amz-date": "20150830T123600Z"},
            s3_server.EMPTY_SHA256,
        )
        self.assertEqual(
            signed.signature,
            "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
        )

    def test_the_s3_range_example_matches_the_s3_reference(self):
        signed = s3_server.sign(
            S3_SECRET,
            "20130524T000000Z",
            "us-east-1",
            "s3",
            "GET",
            "/test.txt",
            [],
            {
                "host": "examplebucket.s3.amazonaws.com",
                "range": "bytes=0-9",
                "x-amz-content-sha256": s3_server.EMPTY_SHA256,
                "x-amz-date": "20130524T000000Z",
            },
            s3_server.EMPTY_SHA256,
        )
        self.assertEqual(
            signed.signature,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
        )

    def test_the_s3_list_example_sorts_and_encodes_the_query(self):
        signed = s3_server.sign(
            S3_SECRET,
            "20130524T000000Z",
            "us-east-1",
            "s3",
            "GET",
            "/",
            [("prefix", "J"), ("max-keys", "2")],
            {
                "host": "examplebucket.s3.amazonaws.com",
                "x-amz-content-sha256": s3_server.EMPTY_SHA256,
                "x-amz-date": "20130524T000000Z",
            },
            s3_server.EMPTY_SHA256,
        )
        self.assertEqual(
            signed.signature,
            "34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7",
        )

    def test_uri_encode_follows_sigv4(self):
        self.assertEqual(s3_server.uri_encode("a/b c+~", encode_slash=False), "a/b%20c%2B~")
        self.assertEqual(s3_server.uri_encode("a/b", encode_slash=True), "a%2Fb")
        self.assertEqual(s3_server.uri_encode("\u1234", encode_slash=True), "%E1%88%B4")


class ServerTest(unittest.TestCase):
    """A server on a free port over a temporary folder, seeded per test."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="azdrive-s3-test-")
        self.server = s3_server.start(
            self.root, access_key=ACCESS, secret_key=SECRET, buckets=[BUCKET]
        )
        self.client = s3_server.Client(self.server.url, ACCESS, SECRET)

    def tearDown(self):
        self.server.stop()
        shutil.rmtree(self.root, ignore_errors=True)

    def seed(self, key, data):
        path = os.path.join(self.root, BUCKET, *key.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)

    def get(self, key, **kw):
        return self.client.request("GET", BUCKET, key, **kw)

    def list(self, **query):
        params = {"list-type": "2"}
        params.update({k.replace("_", "-"): v for k, v in query.items()})
        return self.client.request("GET", BUCKET, "", query=params)


class Listing(ServerTest):
    def setUp(self):
        super().setUp()
        self.seed("mail/inbox/0001.eml", b"first")
        self.seed("mail/inbox/0002.eml", b"second")
        self.seed("mail/sent/0003.eml", b"third")
        self.seed("readme.txt", b"hello")

    def test_a_delimiter_lists_one_folder_level(self):
        status, _, body = self.list(prefix="", delimiter="/")
        self.assertEqual(status, 200, body)
        self.assertEqual(xml_values(body, "Prefix")[1:], ["mail/"])
        self.assertEqual(xml_values(body, "Key"), ["readme.txt"])
        self.assertEqual(xml_values(body, "IsTruncated"), ["false"])

        status, _, body = self.list(prefix="mail/inbox/", delimiter="/")
        self.assertEqual(xml_values(body, "Key"), ["mail/inbox/0001.eml", "mail/inbox/0002.eml"])
        self.assertEqual(xml_values(body, "Size"), ["5", "6"])
        etag = xml_values(body, "ETag")[0]
        self.assertTrue(etag.startswith('"') and etag.endswith('"'), etag)
        modified = xml_values(body, "LastModified")[0]
        self.assertRegex(modified, r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.000Z$")

    def test_no_delimiter_lists_every_key_under_the_prefix(self):
        status, _, body = self.list(prefix="mail/")
        self.assertEqual(
            xml_values(body, "Key"),
            ["mail/inbox/0001.eml", "mail/inbox/0002.eml", "mail/sent/0003.eml"],
        )

    def test_pages_continue_after_the_token_and_skip_a_listed_folder(self):
        seen_keys, seen_prefixes, token, pages = [], [], None, 0
        while True:
            query = {"prefix": "", "delimiter": "/", "max_keys": "1"}
            if token:
                query["continuation_token"] = token
            status, _, body = self.list(**query)
            self.assertEqual(status, 200, body)
            pages += 1
            seen_keys += xml_values(body, "Key")
            seen_prefixes += [
                p for p in xml_values(body, "Prefix")[1:] if p
            ]
            tokens = xml_values(body, "NextContinuationToken")
            if xml_values(body, "IsTruncated") == ["true"]:
                self.assertEqual(len(tokens), 1)
                token = tokens[0]
            else:
                break
        self.assertEqual(pages, 2)
        self.assertEqual(seen_prefixes, ["mail/"])
        self.assertEqual(seen_keys, ["readme.txt"])

    def test_a_missing_bucket_is_404_no_such_bucket(self):
        status, _, body = self.client.request("GET", "nope", "", query={"list-type": "2"})
        self.assertEqual(status, 404)
        self.assertEqual(xml_code(body), "NoSuchBucket")

    def test_virtual_host_style_finds_the_bucket_in_the_host(self):
        client = s3_server.Client(self.server.url, ACCESS, SECRET, path_style=False)
        status, _, body = client.request("GET", BUCKET, "readme.txt")
        self.assertEqual(status, 200, body)
        self.assertEqual(body, b"hello")


class Objects(ServerTest):
    def setUp(self):
        super().setUp()
        self.seed("digits.txt", b"0123456789")

    def test_a_range_answers_206_with_just_those_bytes(self):
        status, headers, body = self.get("digits.txt", headers={"Range": "bytes=2-4"})
        self.assertEqual(status, 206)
        self.assertEqual(body, b"234")
        self.assertEqual(headers.get("content-range"), "bytes 2-4/10")

    def test_an_open_ended_and_a_suffix_range(self):
        self.assertEqual(self.get("digits.txt", headers={"Range": "bytes=7-"})[2], b"789")
        self.assertEqual(self.get("digits.txt", headers={"Range": "bytes=-2"})[2], b"89")
        self.assertEqual(self.get("digits.txt", headers={"Range": "bytes=8-100"})[2], b"89")

    def test_a_range_past_the_end_is_416_invalid_range(self):
        status, _, body = self.get("digits.txt", headers={"Range": "bytes=10-"})
        self.assertEqual(status, 416)
        self.assertEqual(xml_code(body), "InvalidRange")

    def test_a_whole_get_answers_200_with_the_headers(self):
        status, headers, body = self.get("digits.txt")
        self.assertEqual(status, 200)
        self.assertEqual(body, b"0123456789")
        self.assertEqual(headers.get("content-length"), "10")
        self.assertIn("GMT", headers.get("last-modified", ""))

    def test_put_head_get_delete_round_trip(self):
        status, headers, _ = self.client.request("PUT", BUCKET, "docs/a b.txt", body=b"azul")
        self.assertEqual(status, 200)
        self.assertTrue(headers.get("etag", "").startswith('"'))
        with open(os.path.join(self.root, BUCKET, "docs", "a b.txt"), "rb") as f:
            self.assertEqual(f.read(), b"azul")

        status, headers, body = self.client.request("HEAD", BUCKET, "docs/a b.txt")
        self.assertEqual((status, body), (200, b""))
        self.assertEqual(headers.get("content-length"), "4")

        self.assertEqual(self.get("docs/a b.txt")[2], b"azul")

        status, _, _ = self.client.request("DELETE", BUCKET, "docs/a b.txt")
        self.assertEqual(status, 204)
        self.assertFalse(os.path.exists(os.path.join(self.root, BUCKET, "docs", "a b.txt")))
        status, _, _ = self.client.request("DELETE", BUCKET, "docs/a b.txt")
        self.assertEqual(status, 204)

    def test_a_copy_source_header_copies_the_object_inside_the_bucket(self):
        status, _, body = self.client.request(
            "PUT", BUCKET, "backup/digits copy.txt",
            headers={"x-amz-copy-source": "/%s/digits.txt" % BUCKET})
        self.assertEqual(status, 200)
        self.assertEqual(local_name(ET.fromstring(body).tag), "CopyObjectResult")
        self.assertEqual(self.get("backup/digits copy.txt")[2], b"0123456789")
        self.assertEqual(self.get("digits.txt")[2], b"0123456789", "the source stays")
        # The source is URL-encoded, with or without the leading slash.
        self.seed("a b.txt", b"spaced")
        status, _, _ = self.client.request(
            "PUT", BUCKET, "c.txt", headers={"x-amz-copy-source": "%s/a%%20b.txt" % BUCKET})
        self.assertEqual(status, 200)
        self.assertEqual(self.get("c.txt")[2], b"spaced")

    def test_a_copy_of_a_missing_source_is_404_and_writes_nothing(self):
        status, _, body = self.client.request(
            "PUT", BUCKET, "x.txt", headers={"x-amz-copy-source": "/%s/nope.txt" % BUCKET})
        self.assertEqual(status, 404)
        self.assertEqual(xml_code(body), "NoSuchKey")
        self.assertFalse(os.path.exists(os.path.join(self.root, BUCKET, "x.txt")))

    def test_a_missing_key_is_404_no_such_key_and_head_has_no_body(self):
        status, _, body = self.get("nope.txt")
        self.assertEqual(status, 404)
        self.assertEqual(xml_code(body), "NoSuchKey")
        status, _, body = self.client.request("HEAD", BUCKET, "nope.txt")
        self.assertEqual((status, body), (404, b""))

    def test_a_key_that_climbs_out_is_refused_and_nothing_is_written(self):
        status, _, body = self.client.request("PUT", BUCKET, "../escaped.txt", body=b"x")
        self.assertEqual(status, 400, body)
        self.assertFalse(os.path.exists(os.path.join(self.root, "escaped.txt")))
        status, _, _ = self.get("a/../../../etc/passwd")
        self.assertEqual(status, 400)

    def test_a_payload_that_does_not_match_its_hash_is_refused(self):
        status, _, body = self.client.request(
            "PUT", BUCKET, "x.txt", body=b"real", payload_hash=s3_server.sha256_hex(b"other")
        )
        self.assertEqual(status, 400)
        self.assertEqual(xml_code(body), "XAmzContentSHA256Mismatch")
        self.assertFalse(os.path.exists(os.path.join(self.root, BUCKET, "x.txt")))


class Authentication(ServerTest):
    def test_a_wrong_secret_is_403_signature_does_not_match(self):
        client = s3_server.Client(self.server.url, ACCESS, "wrong-secret")
        status, _, body = client.request("GET", BUCKET, "", query={"list-type": "2"})
        self.assertEqual(status, 403)
        self.assertEqual(xml_code(body), "SignatureDoesNotMatch")
        self.assertTrue(xml_values(body, "Message")[0])

    def test_an_unknown_access_key_is_403_invalid_access_key_id(self):
        client = s3_server.Client(self.server.url, "AKIDOTHER", SECRET)
        status, _, body = client.request("GET", BUCKET, "", query={"list-type": "2"})
        self.assertEqual(status, 403)
        self.assertEqual(xml_code(body), "InvalidAccessKeyId")

    def test_an_unsigned_request_is_403_access_denied(self):
        status, _, body = self.client.request("GET", BUCKET, "", query={"list-type": "2"}, sign=False)
        self.assertEqual(status, 403)
        self.assertEqual(xml_code(body), "AccessDenied")

    def test_a_skewed_clock_is_403_request_time_too_skewed(self):
        status, _, body = self.client.request(
            "GET", BUCKET, "", query={"list-type": "2"}, now=time.time() - 3600
        )
        self.assertEqual(status, 403)
        self.assertEqual(xml_code(body), "RequestTimeTooSkewed")

    def test_the_wrong_region_names_the_right_one(self):
        client = s3_server.Client(self.server.url, ACCESS, SECRET, region="eu-west-1")
        status, _, body = client.request("GET", BUCKET, "", query={"list-type": "2"})
        self.assertEqual(status, 400)
        self.assertEqual(xml_code(body), "AuthorizationHeaderMalformed")
        self.assertEqual(xml_values(body, "Region"), ["us-east-1"])


class ConditionalRequests(ServerTest):
    """Conditional writes and reads, as S3 answers them: what a sync's compare-and-swap of its
    index (If-Match / If-None-Match: *) and its polling (If-None-Match: <etag>) need."""

    def setUp(self):
        super().setUp()
        self.seed("index.json", b"{}")
        _, headers, _ = self.client.request("HEAD", BUCKET, "index.json")
        self.etag = headers.get("etag")

    def put(self, key, body, **headers):
        return self.client.request("PUT", BUCKET, key, body=body, headers=headers)

    def test_if_none_match_star_writes_only_a_new_key(self):
        status, _, body = self.put("index.json", b"[]", **{"If-None-Match": "*"})
        self.assertEqual(status, 412)
        self.assertEqual(xml_code(body), "PreconditionFailed")
        self.assertEqual(self.get("index.json")[2], b"{}", "nothing written")
        status, headers, _ = self.put("new.json", b"[]", **{"If-None-Match": "*"})
        self.assertEqual(status, 200)
        self.assertTrue(headers.get("etag", "").startswith('"'))

    def test_if_match_writes_only_over_the_version_read(self):
        status, headers, _ = self.put("index.json", b"[1]", **{"If-Match": self.etag})
        self.assertEqual(status, 200)
        newer = headers.get("etag")
        self.assertNotEqual(newer, self.etag)
        status, _, body = self.put("index.json", b"[2]", **{"If-Match": self.etag})
        self.assertEqual(status, 412, "a stale version loses")
        self.assertEqual(xml_code(body), "PreconditionFailed")
        self.assertEqual(self.get("index.json")[2], b"[1]")
        status, _, _ = self.put("missing.json", b"[3]", **{"If-Match": self.etag})
        self.assertEqual(status, 412, "nothing to match")

    def test_if_none_match_on_a_read_answers_304_for_the_same_version(self):
        status, _, body = self.get("index.json", headers={"If-None-Match": self.etag})
        self.assertEqual((status, body), (304, b""))
        self.put("index.json", b"[1]")
        status, _, body = self.get("index.json", headers={"If-None-Match": self.etag})
        self.assertEqual((status, body), (200, b"[1]"))


class RequestLog(ServerTest):
    def test_the_log_records_every_request_with_its_key(self):
        self.seed("mail/inbox/0001.eml", b"first")
        self.list(prefix="mail/inbox/", delimiter="/")
        self.get("mail/inbox/0001.eml")
        self.client.request("HEAD", BUCKET, "mail/inbox/0001.eml")
        log = self.server.requests()
        self.assertEqual([(r["method"], r["op"]) for r in log], [
            ("GET", "ListObjectsV2"),
            ("GET", "GetObject"),
            ("HEAD", "HeadObject"),
        ])
        self.assertEqual(log[1]["key"], "mail/inbox/0001.eml")
        self.assertEqual(log[1]["status"], 200)
        self.assertEqual(self.server.object_gets(), ["mail/inbox/0001.eml"])


class Versions(unittest.TestCase):
    """A store that keeps versions (the mock stack's: an Azlin node's retention) puts a prefix
    back as it was at a time - what a token server's restore asks a node for."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="s3-versions-")
        self.addCleanup(shutil.rmtree, self.root, True)
        self.store = s3_server.Store(self.root, keep_versions=True)
        self.store.create_bucket(BUCKET)
        self.now = [1000.0]
        self.store.clock = lambda: self.now[0]

    def test_a_restore_puts_the_prefix_back_as_it_was_at_a_time(self):
        self.store.write(BUCKET, "docs/a.txt", b"a1")
        self.store.write(BUCKET, "docs/b.txt", b"b1")
        self.store.write(BUCKET, "other.txt", b"o1")
        self.now[0] = 2000.0
        self.store.write(BUCKET, "docs/a.txt", b"encrypted")
        self.store.delete(BUCKET, "docs/b.txt")
        self.store.write(BUCKET, "docs/note.txt", b"pay")
        self.store.write(BUCKET, "other.txt", b"o2")
        self.now[0] = 3000.0
        self.assertEqual(self.store.restore(BUCKET, "docs/", 1500), 3)
        self.assertEqual(self.store.read(BUCKET, "docs/a.txt"), b"a1")
        self.assertEqual(self.store.read(BUCKET, "docs/b.txt"), b"b1")
        self.assertIsNone(self.store.info(BUCKET, "docs/note.txt"), "made since: gone")
        self.assertEqual(self.store.read(BUCKET, "other.txt"), b"o2", "outside the prefix")
        # Nothing is lost: the objects as they were before the restore come back the same way.
        self.now[0] = 4000.0
        self.assertEqual(self.store.restore(BUCKET, "docs/", 2500), 3)
        self.assertEqual(self.store.read(BUCKET, "docs/a.txt"), b"encrypted")
        self.assertEqual(self.store.restore(BUCKET, "docs/", 2500), 0, "as it was already")

    def test_an_object_from_before_the_store_kept_versions_comes_back_too(self):
        s3_server.Store(self.root).write(BUCKET, "old.txt", b"before")
        os.utime(self.store.path(BUCKET, "old.txt"), (500, 500))
        self.store.write(BUCKET, "old.txt", b"after")
        self.assertEqual(self.store.restore(BUCKET, "", 600), 1)
        self.assertEqual(self.store.read(BUCKET, "old.txt"), b"before")

    def test_a_store_without_versions_refuses_a_restore(self):
        with self.assertRaises(ValueError):
            s3_server.Store(self.root).restore(BUCKET, "", 0)


if __name__ == "__main__":
    unittest.main()
