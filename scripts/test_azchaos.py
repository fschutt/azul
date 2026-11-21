#!/usr/bin/env python3
"""Unit tests of azchaos.py, the fault harness between AzDrive and the local S3 (stdlib only).

    python3 -m unittest discover -s scripts -p 'test_azchaos.py'
    python3 scripts/test_azchaos.py
"""

import email.utils
import hashlib
import http.client
import json
import os
import shutil
import socket
import struct
import sys
import tempfile
import threading
import time
import unittest
import urllib.request
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "examples", "azul-drive", "scripts"))

import azchaos  # noqa: E402
import azlin_client  # noqa: E402
import azlin_mock_stack  # noqa: E402
import s3_server  # noqa: E402

ACCESS = "chaos-test"
SECRET = "chaos-test-secret"
BUCKET = "drive"


def xml_text(body, name):
    root = ET.fromstring(body)
    for element in root.iter():
        if element.tag.rsplit("}", 1)[-1] == name:
            return element.text or ""
    return None


class ProxyCase(unittest.TestCase):
    """An S3 server under a temporary folder and a chaos proxy in front of it."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="azchaos-test-")
        self.server = s3_server.start(self.root, access_key=ACCESS, secret_key=SECRET,
                                      buckets=[BUCKET])
        self.proxy = azchaos.ChaosProxy(self.server.url).start()
        self.client = s3_server.Client(self.proxy.url, ACCESS, SECRET)
        self.direct = s3_server.Client(self.server.url, ACCESS, SECRET)

    def tearDown(self):
        self.proxy.stop()
        self.server.stop()
        shutil.rmtree(self.root, ignore_errors=True)

    def stored(self, key):
        try:
            return self.server.store.read(BUCKET, key)
        except Exception:  # noqa: BLE001 - absent, in whatever way the store says it
            return None

    def put(self, key, body, client=None, headers=None):
        return (client or self.client).request("PUT", BUCKET, key, body=body, headers=headers)


class TheOpOfARequest(unittest.TestCase):
    def test_every_s3_call_azdrive_sends_is_named_as_s3_names_it(self):
        cases = [
            ("GET", "/", {}, {}, ("ListBuckets", "", "")),
            ("GET", "/drive", {"list-type": "2"}, {}, ("ListObjectsV2", "drive", "")),
            ("HEAD", "/drive", {}, {}, ("HeadBucket", "drive", "")),
            ("PUT", "/drive", {}, {}, ("CreateBucket", "drive", "")),
            ("POST", "/drive", {"delete": ""}, {}, ("DeleteObjects", "drive", "")),
            ("GET", "/drive", {"uploads": ""}, {}, ("ListMultipartUploads", "drive", "")),
            ("GET", "/drive/a/b.txt", {}, {}, ("GetObject", "drive", "a/b.txt")),
            ("HEAD", "/drive/a/b.txt", {}, {}, ("HeadObject", "drive", "a/b.txt")),
            ("PUT", "/drive/a/b.txt", {}, {}, ("PutObject", "drive", "a/b.txt")),
            ("PUT", "/drive/a/b.txt", {}, {"x-amz-copy-source": "/drive/c"},
             ("CopyObject", "drive", "a/b.txt")),
            ("PUT", "/drive/big", {"partNumber": "2", "uploadId": "u"}, {},
             ("UploadPart", "drive", "big")),
            ("POST", "/drive/big", {"uploads": ""}, {}, ("CreateMultipartUpload", "drive", "big")),
            ("POST", "/drive/big", {"uploadId": "u"}, {},
             ("CompleteMultipartUpload", "drive", "big")),
            ("GET", "/drive/big", {"uploadId": "u"}, {}, ("ListParts", "drive", "big")),
            ("DELETE", "/drive/big", {"uploadId": "u"}, {},
             ("AbortMultipartUpload", "drive", "big")),
            ("DELETE", "/drive/a/b.txt", {}, {}, ("DeleteObject", "drive", "a/b.txt")),
            ("GET", "/drive/sp%20ace%C3%A9.txt", {}, {}, ("GetObject", "drive", "sp aceé.txt")),
        ]
        for method, path, query, headers, expected in cases:
            with self.subTest(method=method, path=path, query=query):
                self.assertEqual(azchaos.s3_op(method, path, query, headers), expected)


class ThePassThrough(ProxyCase):
    def test_a_signed_put_and_get_go_through_unchanged_and_are_logged(self):
        status, _, _ = self.put("a/one.txt", b"hello")
        self.assertEqual(status, 200)
        status, _, body = self.client.request("GET", BUCKET, "a/one.txt")
        self.assertEqual((status, body), (200, b"hello"))
        ops = [(r["op"], r["key"], r["status"]) for r in self.proxy.requests()]
        self.assertEqual(ops, [("PutObject", "a/one.txt", 200), ("GetObject", "a/one.txt", 200)])
        self.assertEqual(self.proxy.requests()[0]["sent"], 5)
        self.assertEqual(self.proxy.requests()[1]["received"], 5)

    def test_several_requests_on_one_kept_alive_connection_each_pass(self):
        host, port = self.proxy.address
        conn = http.client.HTTPConnection(host, port, timeout=5)
        for _ in range(3):
            conn.request("GET", "/__azchaos/health")
            response = conn.getresponse()
            self.assertEqual(response.status, 200)
            response.read()
        conn.close()

    def test_a_head_answer_has_no_body_and_the_connection_goes_on(self):
        self.put("h.txt", b"12345")
        status, headers, body = self.client.request("HEAD", BUCKET, "h.txt")
        self.assertEqual((status, headers.get("content-length"), body), (200, "5", b""))
        status, _, body = self.client.request("GET", BUCKET, "h.txt")
        self.assertEqual((status, body), (200, b"12345"))


class TheFaults(ProxyCase):
    def test_a_lost_reply_writes_the_object_but_the_client_sees_a_reset(self):
        fault = self.proxy.add(op="PutObject", action="reset_after", times=1)
        with self.assertRaises((ConnectionError, http.client.HTTPException, OSError)):
            self.put("lost.txt", b"landed anyway")
        self.assertEqual(self.stored("lost.txt"), b"landed anyway")
        self.assertEqual(fault.applied, 1)
        # Only once: the retry gets its answer.
        status, _, _ = self.put("lost.txt", b"landed anyway")
        self.assertEqual(status, 200)

    def test_a_reset_before_forwarding_writes_nothing(self):
        self.proxy.add(op="PutObject", action="reset_before")
        with self.assertRaises((ConnectionError, http.client.HTTPException, OSError)):
            self.put("never.txt", b"x")
        self.assertIsNone(self.stored("never.txt"))
        entry = self.proxy.requests()[-1]
        self.assertEqual((entry["op"], entry["fault"], entry["forwarded"]),
                         ("PutObject", "reset_before", False))

    def test_the_nth_matching_request_only_and_only_as_often_as_asked(self):
        self.proxy.add(op="PutObject", key=r"^p", nth=2, action="status", status=503,
                       code="SlowDown", azlin_error="unavailable", retry_after=0)
        answers = [self.put("p%d" % i, b"x")[0] for i in range(4)]
        self.assertEqual(answers, [200, 503, 200, 200])
        self.assertEqual(self.put("q", b"x")[0], 200)

    def test_an_injected_answer_to_a_head_has_no_body_and_the_connection_goes_on(self):
        self.proxy.add(op="HeadObject", action="azlin", azlin_error="credentials_revoked",
                       times=1)
        host, port = self.proxy.address
        sock = socket.create_connection((host, port), timeout=5)
        try:
            sock.sendall(b"HEAD /%s/h.txt HTTP/1.1\r\nHost: x\r\n\r\n" % BUCKET.encode())
            time.sleep(0.3)
            sock.settimeout(0.3)
            data = b""
            try:
                while True:
                    chunk = sock.recv(65536)
                    if not chunk:
                        break
                    data += chunk
            except (socket.timeout, TimeoutError):
                pass
        finally:
            sock.close()
        head, _, rest = data.partition(b"\r\n\r\n")
        self.assertIn(b" 403 ", head.split(b"\r\n")[0])
        self.assertIn(b"x-azlin-error: credentials_revoked", head)
        # A body after a HEAD's head would be read as the next answer on the connection.
        self.assertEqual(rest, b"")

    def test_an_azlin_error_answer_carries_its_s3_code_and_header_and_is_not_forwarded(self):
        self.proxy.add(op="PutObject", action="azlin", azlin_error="quota_exceeded")
        status, headers, body = self.put("full.txt", b"x")
        self.assertEqual(status, 403)
        self.assertEqual(headers.get("x-azlin-error"), "quota_exceeded")
        self.assertEqual(xml_text(body, "Code"), "QuotaExceeded")
        self.assertIsNone(self.stored("full.txt"))

    def test_every_azlin_error_has_the_node_s_status_and_code(self):
        self.assertEqual(azchaos.AZLIN_ERRORS["unavailable"], (503, "SlowDown", 0))
        self.assertEqual(azchaos.AZLIN_ERRORS["maintenance"], (503, "ServiceUnavailable", 30))
        self.assertEqual(azchaos.AZLIN_ERRORS["credentials_revoked"],
                         (403, "InvalidAccessKeyId", None))
        self.assertEqual(azchaos.AZLIN_ERRORS["read_only_unpaid"], (403, "AccessDenied", None))
        self.assertEqual(azchaos.AZLIN_ERRORS["delete_paused"], (503, "SlowDown", 5))

    def test_no_fault_answers_what_a_node_never_sends(self):
        # A 409 with x-azlin-map is no answer of s3d's and no client reads it: a test of it
        # tested the mock only.
        self.assertNotIn("stale_map", azchaos.ACTIONS)
        with self.assertRaises(ValueError):
            self.proxy.add(op="UploadPart", action="stale_map", epoch=42)

    def test_a_cut_body_sends_part_of_the_object_then_resets(self):
        self.put("cut.bin", bytes(range(256)) * 64)
        self.proxy.add(op="GetObject", action="cut_body", bytes=1000)
        host, port = self.proxy.address
        sock = socket.create_connection((host, port), timeout=5)
        status, headers, body = None, None, b""
        try:
            path = "/%s/cut.bin" % BUCKET
            request = self.client_request_bytes("GET", path)
            sock.sendall(request)
            data = b""
            while True:
                try:
                    chunk = sock.recv(65536)
                except ConnectionResetError:
                    break
                if not chunk:
                    break
                data += chunk
            head, _, body = data.partition(b"\r\n\r\n")
            status = int(head.split(b" ")[1])
        finally:
            sock.close()
        self.assertEqual(status, 200)
        self.assertEqual(len(body), 1000)

    def client_request_bytes(self, method, path):
        """A signed request as raw bytes (through s3_server's signer, captured)."""
        captured = {}

        class Capture(s3_server.Client):
            def send(self, method, target, host, send, body):
                captured["head"] = "%s %s HTTP/1.1\r\nHost: %s\r\n%sContent-Length: 0\r\n\r\n" % (
                    method, target, host,
                    "".join("%s: %s\r\n" % (k, v) for k, v in send.items()))
                return 0, {}, b""

        bucket, _, key = path.lstrip("/").partition("/")
        Capture(self.proxy.url, ACCESS, SECRET).request(method, bucket, key)
        return captured["head"].encode("ascii")

    def test_a_stall_holds_the_request_until_released(self):
        self.proxy.add(op="PutObject", action="stall")
        done = {}

        def put():
            done["status"] = self.put("held.txt", b"x")[0]

        worker = threading.Thread(target=put)
        worker.start()
        deadline = time.time() + 5
        while self.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.02)
        self.assertEqual(self.proxy.held(), 1)
        self.assertIsNone(self.stored("held.txt"))
        self.proxy.clear()
        self.proxy.release()
        worker.join(5)
        self.assertEqual(done.get("status"), 200)
        self.assertEqual(self.stored("held.txt"), b"x")

    def test_a_stall_after_forwarding_holds_the_answer_of_a_write_that_landed(self):
        self.proxy.add(op="PutObject", action="stall_after", seconds=0.4)
        started = time.time()
        status = self.put("slow-answer.txt", b"x")[0]
        self.assertEqual(status, 200)
        self.assertGreaterEqual(time.time() - started, 0.35)

    def test_latency_delays_each_matching_request(self):
        self.proxy.add(op="GetObject", action="delay", ms=300)
        self.put("d.txt", b"x")
        started = time.time()
        self.client.request("GET", BUCKET, "d.txt")
        self.assertGreaterEqual(time.time() - started, 0.28)

    def test_a_corrupted_body_differs_in_one_byte(self):
        self.put("c.bin", b"\x00" * 100)
        self.proxy.add(op="GetObject", action="corrupt")
        _, _, body = self.client.request("GET", BUCKET, "c.bin")
        self.assertEqual(len(body), 100)
        self.assertEqual(sum(1 for b in body if b != 0), 1)

    def test_a_probability_fault_with_a_seed_is_repeatable(self):
        first = azchaos.Fault(action="reset_before", probability=0.5, seed=7)
        second = azchaos.Fault(action="reset_before", probability=0.5, seed=7)
        rolls = [first.rolls() for _ in range(20)]
        self.assertEqual(rolls, [second.rolls() for _ in range(20)])
        self.assertIn(True, rolls)
        self.assertIn(False, rolls)

    def test_a_fault_matches_by_the_client_s_session_token(self):
        self.proxy.add(op="PutObject", client="laptop-token", action="azlin",
                       azlin_error="credentials_revoked")
        mine = self.put("t1", b"x", headers={"x-amz-security-token": "laptop-token-1"})[0]
        theirs = self.put("t2", b"x", headers={"x-amz-security-token": "desktop-token-1"})[0]
        self.assertEqual((mine, theirs), (403, 200))


class TheTokenRoutes(unittest.TestCase):
    """The token server's routes are named as the router names them, so a rule hits one route."""

    def test_every_token_route_the_apps_call_has_its_op_and_its_id_as_the_key(self):
        cases = [
            ("POST", "/v1/drives/d_abc/credentials", ("Refresh", "d_abc")),
            ("POST", "/v1/drives/d_abc/lockdown", ("Lockdown", "d_abc")),
            ("POST", "/v1/drives/d_abc/lockdown/cancel", ("LockdownCancel", "d_abc")),
            ("POST", "/v1/drives/d_abc/claim", ("Claim", "d_abc")),
            ("POST", "/v1/drives/d_abc/redeem", ("Redeem", "d_abc")),
            ("POST", "/v1/vouchers/redeem", ("Voucher", "")),
            ("POST", "/v1/tokens/issue", ("Issue", "")),
            ("GET", "/v1/tokens/keys", ("IssuerKeys", "")),
            ("POST", "/v1/checkout", ("CheckoutCreate", "")),
            ("GET", "/v1/checkout/options?tier=1TB", ("CheckoutOptions", "")),
            ("GET", "/v1/checkout/ck_x", ("CheckoutStatus", "ck_x")),
            ("POST", "/v1/checkout/ck_x/pay", ("CheckoutPay", "ck_x")),
            ("POST", "/v1/checkout/ck_x/surface", ("CheckoutSurface", "ck_x")),
            ("POST", "/v1/checkout/ck_x/abandon", ("Abandon", "ck_x")),
            ("POST", "/v1/checkout/ck_x/cancel", ("CheckoutCancel", "ck_x")),
            ("POST", "/v1/drives/d_abc/members", ("AddMember", "d_abc")),
            ("DELETE", "/v1/drives/d_abc/members/m_1", ("RemoveMember", "d_abc")),
            ("POST", "/v1/drives", ("Signup", "")),
            ("GET", "/v1/drives/d_abc", ("DriveInfo", "d_abc")),
            ("POST", "/v1/drives/d_abc/keys", ("CreateKey", "d_abc")),
            ("DELETE", "/v1/drives/d_abc/keys/AZK1", ("RevokeKey", "d_abc")),
            ("POST", "/v1/drives/d_abc/restore", ("Restore", "d_abc")),
            ("GET", "/v1/drives/d_abc/restore/r_1", ("RestoreStatus", "d_abc")),
            ("POST", "/v1/drives/d_abc/recovery", ("SetRecovery", "d_abc")),
            ("GET", "/v1/drives/d_abc/recovery_keys", ("RecoveryKeys", "d_abc")),
            ("POST", "/v1/drives/d_abc/recovery_keys", ("AddRecoveryKey", "d_abc")),
            ("DELETE", "/v1/drives/d_abc/recovery_keys/rk_1", ("RemoveRecoveryKey", "d_abc")),
            ("POST", "/v1/recovery/challenge", ("RecoveryChallenge", "")),
            ("POST", "/v1/recovery/lookup", ("RecoveryLookup", "")),
            ("GET", "/v1/tiers?country=DE", ("Tiers", "")),
            ("GET", "/health", ("Health", "")),
            ("GET", "/v1/health", ("Health", "")),
            ("POST", "/v1/webhook/fake-stripe", ("Webhook", "")),
            ("GET", "/v1/somewhere/else", ("TokenOther", "")),
        ]
        for method, target, want in cases:
            path = target.split("?", 1)[0]
            self.assertEqual(azchaos.token_op(method, path), want, (method, target))

    def test_a_proxy_of_either_kind_names_requests_by_what_is_behind_it(self):
        head = b"POST /v1/drives/d_abc/credentials HTTP/1.1\r\nHost: x\r\n\r\n"
        self.assertEqual(azchaos.Request(head, "token").op, "Refresh")
        # "v1" is no bucket name (S3 names have three letters at least): the auto kind tells a
        # token route from an S3 call by it.
        self.assertEqual(azchaos.Request(head, "auto").op, "Refresh")
        self.assertEqual(azchaos.Request(head, "s3").op, "PostObject")
        health = b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n"
        self.assertEqual(azchaos.Request(health, "token").op, "Health")
        self.assertEqual(azchaos.Request(health, "auto").op, "ListObjects")
        with self.assertRaises(ValueError):
            azchaos.ChaosProxy("http://127.0.0.1:1", kind="dns")


class TokenProxyCase(unittest.TestCase):
    """The mock stack's token server and a chaos proxy in front of it."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="azchaos-token-test-")
        self.stack = azlin_mock_stack.start(os.path.join(self.root, "s3"))
        self.state = self.stack.token.state
        self.proxy = azchaos.ChaosProxy(self.stack.token_url, kind="token").start()
        self.client = azlin_client.TokenClient(self.proxy.url, timeout=10)
        bundle = self.state.signup({"name": "chaos"})
        self.drive = bundle["drive"]["id"]
        self.token = bundle["drive_token"]
        self.family = self.token[3:].split(".")[0]

    def tearDown(self):
        self.proxy.stop()
        self.stack.stop()
        shutil.rmtree(self.root, ignore_errors=True)

    def generation(self):
        return self.state.families[self.family]["generation"]

    def refresh(self, token=None):
        return self.client.refresh(self.drive, token or self.token)


class TheTokenFaults(TokenProxyCase):
    def test_a_lost_refresh_answer_rotates_at_the_server_and_the_client_sees_a_reset(self):
        rule = self.proxy.add(op="Refresh", action="reset_after")
        with self.assertRaises((OSError, http.client.HTTPException)):
            self.refresh()
        self.assertEqual(self.generation(), 1, "the server rotated the token")
        self.assertEqual(rule.applied, 1)
        entry = self.proxy.requests(op="Refresh")[0]
        self.assertEqual((entry["forwarded"], entry["upstream_status"], entry["key"]),
                         (True, 200, self.drive))

    def test_an_answer_replaced_after_the_write_landed_is_the_given_refusal(self):
        # The server committed the rotation, the client hears a 503 (T2's case).
        self.proxy.add(op="Refresh", action="answer_after", status=503, code="unavailable",
                       fields={"spent": True}, times=1)
        status, value, _ = self.refresh()
        self.assertEqual(status, 503)
        self.assertEqual(value, {"error": "unavailable", "message": "azchaos", "spent": True})
        self.assertEqual(self.generation(), 1)
        entry = self.proxy.requests(op="Refresh")[0]
        self.assertEqual((entry["upstream_status"], entry["status"]), (200, 503))

    def test_a_status_on_a_token_route_answers_its_json_and_forwards_nothing(self):
        self.proxy.add(op="Refresh", action="status", status=503, code="try_again",
                       fields={"spent": False}, times=1)
        status, value, _ = self.refresh()
        self.assertEqual((status, value["error"], value["spent"]), (503, "try_again", False))
        self.assertEqual(self.generation(), 0, "nothing reached the server")
        status, value, _ = self.refresh()
        self.assertEqual(status, 200)
        self.assertEqual(self.generation(), 1)

    def test_a_scripted_body_is_answered_as_given(self):
        self.proxy.add(op="Abandon", action="status", status=200,
                       body={"checkout_id": "ck_x", "status": "pending"})
        status, value, _ = self.client.call("POST", "/v1/checkout/ck_x/abandon", {})
        self.assertEqual((status, value), (200, {"checkout_id": "ck_x", "status": "pending"}))

    def test_a_rule_with_a_drive_id_takes_only_that_drive_s_calls(self):
        other = self.state.signup({"name": "other"})
        self.proxy.add(op="Refresh", key="^%s$" % other["drive"]["id"], action="status",
                       status=503, code="unavailable")
        self.assertEqual(self.refresh()[0], 200)
        status, _, _ = self.client.refresh(other["drive"]["id"], other["drive_token"])
        self.assertEqual(status, 503)

    def test_a_stall_after_holds_the_refresh_answer_until_released(self):
        self.proxy.add(op="Refresh", action="stall_after", times=1)
        got = {}
        thread = threading.Thread(target=lambda: got.update(answer=self.refresh()))
        thread.start()
        deadline = time.time() + 5
        while self.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.01)
        self.assertEqual(self.proxy.held(), 1)
        self.assertEqual(self.generation(), 1, "the rotation landed before the answer was held")
        self.proxy.release()
        thread.join(5)
        self.assertEqual(got["answer"][0], 200)

    def test_the_log_names_the_family_and_never_holds_a_drive_token(self):
        self.refresh()
        entry = self.proxy.requests(op="Refresh")[0]
        self.assertEqual(entry["client"], "dt_" + self.family)
        secret = self.token.split(".", 2)[2]
        self.assertNotIn(secret, json.dumps(self.proxy.requests()))


class TheNetwork(ProxyCase):
    def test_down_resets_every_connection_and_up_lets_them_through_again(self):
        self.proxy.network("down")
        with self.assertRaises((ConnectionError, http.client.HTTPException, OSError)):
            self.put("x.txt", b"x")
        self.proxy.network("up")
        self.assertEqual(self.put("x.txt", b"x")[0], 200)

    def test_a_blackhole_takes_requests_and_never_answers(self):
        self.proxy.network("blackhole")
        host, port = self.proxy.address
        conn = http.client.HTTPConnection(host, port, timeout=0.5)
        with self.assertRaises((socket.timeout, TimeoutError, OSError)):
            conn.request("GET", "/%s/x" % BUCKET)
            conn.getresponse()
        conn.close()
        self.proxy.network("up")

    def test_going_down_kills_a_transfer_in_flight(self):
        self.proxy.add(op="PutObject", action="stall")
        errors = []

        def put():
            try:
                self.put("inflight.txt", b"x")
            except (ConnectionError, http.client.HTTPException, OSError) as e:
                errors.append(e)

        worker = threading.Thread(target=put)
        worker.start()
        deadline = time.time() + 5
        while self.proxy.held() == 0 and time.time() < deadline:
            time.sleep(0.02)
        self.proxy.network("down")
        worker.join(5)
        self.assertEqual(len(errors), 1)
        self.proxy.clear()
        self.proxy.network("up")

    def test_a_flap_alternates_down_and_up(self):
        self.proxy.flap(up=0.2, down=0.2)
        seen = set()
        deadline = time.time() + 2
        while time.time() < deadline and len(seen) < 2:
            seen.add(self.proxy.state())
            time.sleep(0.03)
        self.proxy.stop_flap()
        self.assertEqual(seen, {"up", "down"})
        self.assertEqual(self.proxy.state(), "up")


class TheServerSide(ProxyCase):
    def test_a_server_clock_ahead_refuses_requests_as_too_skewed_and_says_its_time(self):
        self.proxy.clock_offset = 20 * 60
        status, headers, body = self.put("skew.txt", b"x")
        self.assertEqual(status, 403)
        self.assertEqual(xml_text(body, "Code"), "RequestTimeTooSkewed")
        served = email.utils.parsedate_to_datetime(headers["date"]).timestamp()
        self.assertAlmostEqual(served, time.time() + 20 * 60, delta=5)
        self.proxy.clock_offset = 0
        self.assertEqual(self.put("skew.txt", b"x")[0], 200)

    def test_a_request_signed_now_is_within_a_node_clock_a_minute_ahead(self):
        # The signed time is UTC: a local-time reading would be off by the zone (and by daylight
        # saving time), and every request would look an hour skewed.
        self.proxy.clock_offset = 60
        self.assertEqual(self.put("on-time.txt", b"x")[0], 200)

    def test_a_quota_refuses_the_write_that_crosses_it_and_reads_go_on(self):
        self.proxy.set_quota(BUCKET, 10, used=lambda: self.server.store.stored_bytes(BUCKET))
        self.assertEqual(self.put("a", b"123456")[0], 200)
        status, headers, _ = self.put("b", b"123456")
        self.assertEqual((status, headers.get("x-azlin-error")), (403, "quota_exceeded"))
        self.assertEqual(self.client.request("GET", BUCKET, "a")[0], 200)
        self.proxy.set_quota(BUCKET, None)
        self.assertEqual(self.put("b", b"123456")[0], 200)

    def test_a_sink_takes_a_multipart_upload_keeps_its_hash_and_stores_nothing(self):
        sink = self.proxy.sink(r"^blobs/")
        status, _, body = self.client.request("POST", BUCKET, "blobs/big", query={"uploads": ""})
        self.assertEqual(status, 200)
        upload = xml_text(body, "UploadId")
        parts = [b"a" * 1000, b"b" * 500]
        etags = []
        for number, data in enumerate(parts, 1):
            status, headers, _ = self.client.request(
                "PUT", BUCKET, "blobs/big", query={"partNumber": str(number), "uploadId": upload},
                body=data)
            self.assertEqual(status, 200)
            etags.append(headers["etag"].strip('"'))
            self.assertEqual(etags[-1], hashlib.md5(data).hexdigest())
        status, _, body = self.client.request("GET", BUCKET, "blobs/big",
                                              query={"uploadId": upload})
        self.assertEqual((status, xml_text(body, "PartNumber")), (200, "1"))
        complete = "<CompleteMultipartUpload>%s</CompleteMultipartUpload>" % "".join(
            "<Part><PartNumber>%d</PartNumber><ETag>\"%s\"</ETag></Part>" % (i + 1, e)
            for i, e in enumerate(etags))
        status, _, _ = self.client.request("POST", BUCKET, "blobs/big",
                                           query={"uploadId": upload},
                                           body=complete.encode("ascii"))
        self.assertEqual(status, 200)
        status, headers, _ = self.client.request("HEAD", BUCKET, "blobs/big")
        self.assertEqual((status, headers.get("content-length")), (200, "1500"))
        self.assertEqual(sink.objects["blobs/big"]["sha256"],
                         hashlib.sha256(b"".join(parts)).hexdigest())
        self.assertEqual(sink.objects["blobs/big"]["size"], 1500)
        self.assertIsNone(self.stored("blobs/big"))


class TheCredentials(ProxyCase):
    def test_an_authorize_hook_refuses_a_device_s_expired_or_revoked_session(self):
        def authorize(request):
            token = request.header("x-amz-security-token") or ""
            if token.startswith("expired"):
                return (400, "ExpiredToken", None)
            if token.startswith("revoked"):
                return (403, "InvalidAccessKeyId", "credentials_revoked")
            return None

        self.proxy.authorize = authorize
        status, _, body = self.put("e", b"x", headers={"x-amz-security-token": "expired-1"})
        self.assertEqual((status, xml_text(body, "Code")), (400, "ExpiredToken"))
        status, headers, _ = self.put("r", b"x", headers={"x-amz-security-token": "revoked-1"})
        self.assertEqual((status, headers.get("x-azlin-error")), (403, "credentials_revoked"))
        self.assertEqual(self.put("ok", b"x", headers={"x-amz-security-token": "fresh"})[0], 200)
        self.assertIsNone(self.stored("e"))
        self.assertEqual(self.proxy.requests()[0]["fault"], "credentials")


class TheControl(ProxyCase):
    def control(self, body):
        request = urllib.request.Request(self.proxy.control_url + "/__azchaos/", method="POST",
                                         data=json.dumps(body).encode("utf-8"))
        with urllib.request.urlopen(request, timeout=5) as response:
            return json.loads(response.read().decode("utf-8"))

    def test_another_process_steers_the_proxy_over_http(self):
        self.control({"add": {"op": "PutObject", "action": "azlin",
                              "azlin_error": "maintenance"}})
        self.assertEqual(self.put("m", b"x")[0], 503)
        self.control({"clear": True})
        self.assertEqual(self.put("m", b"x")[0], 200)
        answer = self.control({"network": "down"})
        self.assertEqual(answer["state"], "down")
        self.control({"network": "up"})
        with urllib.request.urlopen(self.proxy.control_url + "/__azchaos/log",
                                    timeout=5) as response:
            log = json.loads(response.read().decode("utf-8"))
        self.assertEqual([e["status"] for e in log if e["op"] == "PutObject"], [503, 200])

    def test_the_command_line_names_its_switches(self):
        self.assertIn("--upstream", azchaos.USAGE)
        self.assertIn("--rules", azchaos.USAGE)


class TheLaneAndDns(unittest.TestCase):
    def test_a_udp_sink_swallows_every_datagram(self):
        sink = azchaos.UdpSink().start()
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.settimeout(0.3)
            for _ in range(3):
                sock.sendto(b"quic-initial", sink.address)
            with self.assertRaises((socket.timeout, TimeoutError)):
                sock.recvfrom(100)
            deadline = time.time() + 2
            while sink.datagrams < 3 and time.time() < deadline:
                time.sleep(0.02)
            self.assertEqual(sink.datagrams, 3)
            sock.close()
        finally:
            sink.stop()

    def test_the_slow_dns_server_answers_an_a_record_after_its_delay(self):
        dns = azchaos.SlowDns({"node.azchaos.test.": "127.0.0.1"}, delay=0.3).start()
        try:
            query = struct.pack(">HHHHHH", 0x1234, 0x0100, 1, 0, 0, 0)
            for label in ("node", "azchaos", "test"):
                query += bytes([len(label)]) + label.encode("ascii")
            query += b"\x00" + struct.pack(">HH", 1, 1)
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.settimeout(3)
            started = time.time()
            sock.sendto(query, ("127.0.0.1", dns.port))
            answer, _ = sock.recvfrom(512)
            self.assertGreaterEqual(time.time() - started, 0.28)
            self.assertEqual(answer[:2], b"\x12\x34")
            self.assertEqual(struct.unpack(">H", answer[6:8])[0], 1)
            self.assertEqual(answer[-4:], bytes([127, 0, 0, 1]))
            sock.close()
        finally:
            dns.stop()

    def test_a_process_with_the_test_resolver_asks_the_slow_server_for_the_test_names_only(self):
        folder = tempfile.mkdtemp(prefix="azchaos-resolver-")
        dns = azchaos.SlowDns({"n1.azchaos.test": "127.0.0.9"}, delay=0.3).start()
        try:
            resolver = azchaos.TestResolver(folder, "azchaos.test")
            env = dict(os.environ, **resolver.env(dns.port, wait=3))
            probe = "\n".join([
                "import socket, time",
                "started = time.time()",
                "found = socket.getaddrinfo('n1.azchaos.test', 443, type=socket.SOCK_STREAM)",
                "print(found[0][4], round(time.time() - started, 1))",
                "print(socket.getaddrinfo('localhost', 80, socket.AF_INET)[0][4][0])",
                "try:",
                "    socket.getaddrinfo('other.azchaos.test', 80)",
                "except socket.gaierror:",
                "    print('unknown')",
            ])
            import subprocess  # noqa: PLC0415
            ran = subprocess.run([sys.executable, "-c", probe], env=env, capture_output=True,
                                 text=True, timeout=30, check=False)
            self.assertEqual(ran.returncode, 0, ran.stderr)
            lines = ran.stdout.split("\n")
            self.assertEqual(lines[0], "('127.0.0.9', 443) 0.3", ran.stdout)
            self.assertEqual(lines[1], "127.0.0.1")
            self.assertEqual(lines[2], "unknown")
            self.assertEqual(dns.queries, ["n1.azchaos.test.", "other.azchaos.test."])
        finally:
            dns.stop()
            shutil.rmtree(folder, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
