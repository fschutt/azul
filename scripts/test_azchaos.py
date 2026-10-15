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
        self.put("h.txt", b"12345")
        self.proxy.add(op="HeadObject", action="azlin", azlin_error="credentials_revoked",
                       times=1)
        host, port = self.proxy.address
        conn = http.client.HTTPConnection(host, port, timeout=5)
        conn.request("HEAD", "/%s/h.txt" % BUCKET)
        response = conn.getresponse()
        self.assertEqual((response.status, response.read()), (403, b""))
        self.assertEqual(response.getheader("x-azlin-error"), "credentials_revoked")
        # The next answer on the same connection is read as an answer, not as the HEAD's body.
        conn.request("GET", "/__azchaos/health")
        self.assertEqual(conn.getresponse().status, 200)
        conn.close()

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

    def test_a_stale_map_answer_is_409_with_the_newer_epoch_and_its_hash(self):
        self.proxy.add(op="UploadPart", action="stale_map", epoch=42)
        status, headers, _ = self.client.request("PUT", BUCKET, "big",
                                                 query={"partNumber": "1", "uploadId": "u"},
                                                 body=b"part")
        self.assertEqual(status, 409)
        epoch, digest = headers["x-azlin-map"].split(" ")
        self.assertEqual(epoch, "42")
        self.assertEqual(len(digest), 64)

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


if __name__ == "__main__":
    unittest.main()
