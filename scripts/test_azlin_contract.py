#!/usr/bin/env python3
"""The Python tools read the contract crate's golden fixtures (examples/azlin-contract/fixtures)
instead of keeping copies of their own: the fault proxy's node errors, the mock stack's node
answers, the idempotency key and the client's hello (stdlib only).

    python3 -m unittest discover -s scripts -p 'test_azlin_contract.py'
"""

import json
import os
import shutil
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import azchaos  # noqa: E402
import azlin_client  # noqa: E402
import azlin_contract  # noqa: E402
import azlin_mock_stack  # noqa: E402

FIXTURES = os.path.join(HERE, "..", "examples", "azlin-contract", "fixtures")


def fixture(name):
    with open(os.path.join(FIXTURES, name), encoding="utf-8") as f:
        return json.load(f)


class TheContract(unittest.TestCase):
    def test_the_node_errors_are_the_fixture_s_every_one(self):
        table = {e["code"]: (e["status"], e["s3"], e["retry_after"])
                 for e in fixture("azlin_errors.json")}
        self.assertEqual(azlin_contract.azlin_errors(), table)
        self.assertEqual(azchaos.AZLIN_ERRORS, table, "the fault proxy keeps no copy")

    def test_the_idempotency_key_is_base64url_of_sixteen_bytes(self):
        self.assertEqual(azlin_contract.idempotency_key(bytes(range(16))),
                         fixture("idempotency_key.json"))
        fresh = azlin_contract.idempotency_key()
        self.assertEqual(len(fresh), 22)
        self.assertNotEqual(fresh, azlin_contract.idempotency_key())
        self.assertEqual(azlin_contract.IDEMPOTENCY_HEADER, "x-azlin-idempotency")

    def test_the_hello_names_the_crate_s_contract_version(self):
        version = azlin_contract.contract_version()
        for example in fixture("client_hello.json"):
            self.assertTrue(example.endswith("; contract=%d" % version), example)
        self.assertEqual(azlin_contract.client_hello("azcloud", "1.2.3"),
                         "azcloud/1.2.3; contract=%d" % version)
        self.assertEqual(azlin_contract.CLIENT_HEADER, "x-azlin-client")


class TheMockStack(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="azlin-contract-test-")
        self.stack = azlin_mock_stack.start(os.path.join(self.root, "s3"))

    def tearDown(self):
        self.stack.stop()
        shutil.rmtree(self.root, ignore_errors=True)

    def test_its_node_answers_are_the_contract_s(self):
        errors = azlin_contract.azlin_errors()
        s3 = self.stack.s3
        for code in ("quota_exceeded", "drive_banned"):
            status, s3_code, _ = errors[code]
            self.assertEqual(s3.node_error(code)[:2], (status, s3_code), code)

    def test_it_logs_each_call_s_hello_and_idempotency_key_and_needs_neither(self):
        client = azlin_client.TokenClient(self.stack.token_url)
        hello = azlin_contract.client_hello("azcloud", "1.2.3")
        key = azlin_contract.idempotency_key()
        status, bundle, _ = client.call("POST", "/v1/drives", {"name": "x"},
                                        headers={azlin_contract.CLIENT_HEADER: hello})
        self.assertEqual(status, 201)
        drive, token = bundle["drive"]["id"], bundle["drive_token"]
        status, _, _ = client.call("POST", "/v1/drives/%s/lockdown" % drive, {}, bearer=token,
                                   headers={azlin_contract.IDEMPOTENCY_HEADER: key})
        self.assertEqual(status, 200, "an old client without a hello still works")
        calls = self.stack.token.requests()
        self.assertEqual(calls[-2]["client"], hello)
        self.assertEqual(calls[-1]["client"], None)
        self.assertEqual(calls[-1]["idempotency"], key)


if __name__ == "__main__":
    unittest.main()
