#!/usr/bin/env python3
"""Tests for imap_server.py, the stdlib IMAP test server AzMail's end-to-end test syncs from.

Run from anywhere:

    python3 examples/azul-mail/scripts/test_imap_server.py -v

The client side is Python's own imaplib, so the server is checked against an independent IMAP
implementation, not against itself.
"""
import base64
import imaplib
import json
import os
import re
import shutil
import socket
import ssl
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import imap_server  # noqa: E402  (the module under test, next to this file)

USER = 'ada@example.org'
PASSWORD = 'test-password-1'

MESSAGE = """Message-ID: <m{n}@example.org>
Date: Wed, 30 Sep 2026 10:4{n}:00 +0200
From: Ben <ben@example.org>
To: Ada <ada@example.org>
Subject: Message {n}

Body of message {n}.
"""


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        f.write(text)


class ServerTest(unittest.TestCase):
    """Starts a server on a free port over a folder of mailboxes, for each test."""

    tls = False

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix='azmail-imap-test-')
        self.addCleanup(shutil.rmtree, self.root, True)
        for n in (1, 2, 3):
            write(os.path.join(self.root, 'INBOX', f'000{n}.eml'), MESSAGE.format(n=n))
        write(os.path.join(self.root, 'Spam', '0001.eml'), MESSAGE.format(n=7))
        write(os.path.join(self.root, 'Entwürfe', '0001.eml'), MESSAGE.format(n=8))
        write(os.path.join(self.root, 'Work', 'Projects', '0001.eml'), MESSAGE.format(n=9))
        self.log = os.path.join(self.root, 'server.log')
        self.ca = None
        context = None
        if self.tls:
            self.ca, key = imap_server.make_self_signed_cert(self.root)
            context = imap_server.server_ssl_context(self.ca, key)
        self.server = imap_server.start(
            self.root, '127.0.0.1', 0, USER, PASSWORD, log_path=self.log, ssl_context=context)
        self.addCleanup(self.server.shutdown)
        self.addCleanup(self.server.server_close)
        self.port = self.server.server_address[1]

    def connect(self):
        if self.tls:
            context = ssl.create_default_context(cafile=self.ca)
            client = imaplib.IMAP4_SSL('127.0.0.1', self.port, ssl_context=context)
        else:
            client = imaplib.IMAP4('127.0.0.1', self.port)
        self.addCleanup(self.close, client)
        return client

    @staticmethod
    def close(client):
        try:
            client.logout()
        except Exception:
            pass

    def signed_in(self):
        client = self.connect()
        client.login(USER, PASSWORD)
        return client

    def raw(self, *lines):
        """Sends raw command lines and returns everything the server answered until the last
        command's tagged answer."""
        sock = socket.create_connection(('127.0.0.1', self.port), timeout=5)
        self.addCleanup(sock.close)
        data = b''
        while b'\r\n' not in data:
            data += sock.recv(4096)
        answer = b''
        for line in lines:
            sock.sendall(line.encode() + b'\r\n')
        last_tag = lines[-1].split(' ', 1)[0].encode()
        while not re.search(rb'(^|\r\n)' + re.escape(last_tag) + rb' (OK|NO|BAD)[^\r\n]*\r\n', answer):
            chunk = sock.recv(4096)
            if not chunk:
                break
            answer += chunk
        return answer.decode('utf-8', 'replace')

    def log_entries(self):
        with open(self.log, encoding='utf-8') as f:
            return [json.loads(line) for line in f if line.strip()]


class TestServer(ServerTest):
    def test_the_greeting_and_capabilities_offer_plain_and_xoauth2(self):
        client = self.connect()
        self.assertIn('IMAP4REV1', client.capabilities)
        self.assertIn('AUTH=PLAIN', client.capabilities)
        self.assertIn('AUTH=XOAUTH2', client.capabilities)

    def test_a_wrong_password_is_refused(self):
        client = self.connect()
        with self.assertRaises(imaplib.IMAP4.error):
            client.login(USER, 'wrong')

    def test_login_and_authenticate_plain_sign_in(self):
        self.assertEqual(self.signed_in().state, 'AUTH')
        client = self.connect()
        typ, _ = client.authenticate('PLAIN', lambda _: f'\0{USER}\0{PASSWORD}'.encode())
        self.assertEqual(typ, 'OK')

    def test_xoauth2_takes_the_password_as_the_token(self):
        client = self.connect()
        token = f'user={USER}\x01auth=Bearer {PASSWORD}\x01\x01'.encode()
        typ, _ = client.authenticate('XOAUTH2', lambda _: token)
        self.assertEqual(typ, 'OK')
        wrong = self.connect()
        bad = f'user={USER}\x01auth=Bearer nope\x01\x01'.encode()
        # The server answers a failed XOAUTH2 with an error challenge first; the client answers
        # it with an empty line (imaplib calls the callback again).
        answers = iter([bad, b''])
        with self.assertRaises(imaplib.IMAP4.error):
            wrong.authenticate('XOAUTH2', lambda _: next(answers))

    def test_commands_before_sign_in_are_refused(self):
        answer = self.raw('a1 SELECT INBOX', 'a2 LIST "" *')
        self.assertRegex(answer, r'a1 (BAD|NO) ')
        self.assertRegex(answer, r'a2 (BAD|NO) ')
        self.assertNotIn('* LIST', answer)

    def test_list_shows_special_use_and_modified_utf7_names(self):
        typ, data = self.signed_in().list('""', '*')
        self.assertEqual(typ, 'OK')
        lines = [d.decode() for d in data]
        by_name = {re.search(r'"([^"]*)"$', line).group(1): line for line in lines}
        self.assertEqual(
            sorted(by_name), ['Entw&APw-rfe', 'INBOX', 'Spam', 'Work', 'Work/Projects'])
        self.assertIn('\\Junk', by_name['Spam'])
        self.assertNotIn('\\Junk', by_name['INBOX'])
        self.assertIn('\\HasChildren', by_name['Work'])
        self.assertIn('"/"', by_name['INBOX'])

    def test_select_and_examine_report_exists_uidvalidity_and_uidnext(self):
        client = self.signed_in()
        typ, data = client.select('INBOX')
        self.assertEqual((typ, data), ('OK', [b'3']))
        self.assertEqual(client.response('UIDVALIDITY')[1], [b'1'])
        self.assertEqual(client.response('UIDNEXT')[1], [b'4'])
        typ, data = client.select('Entw&APw-rfe', readonly=True)
        self.assertEqual((typ, data), ('OK', [b'1']))
        typ, _ = client.select('Nowhere')
        self.assertEqual(typ, 'NO')

    def test_uid_search_follows_rfc_3501_star_semantics(self):
        client = self.signed_in()
        client.select('INBOX', readonly=True)
        self.assertEqual(client.uid('SEARCH', 'UID', '2:*'), ('OK', [b'2 3']))
        # `n:*` with n above every UID still matches the newest message.
        self.assertEqual(client.uid('SEARCH', 'UID', '9:*'), ('OK', [b'3']))
        self.assertEqual(client.uid('SEARCH', 'ALL'), ('OK', [b'1 2 3']))

    def test_uid_fetch_gives_metadata_and_the_exact_bytes(self):
        client = self.signed_in()
        client.select('INBOX', readonly=True)
        typ, data = client.uid('FETCH', '1:*', '(UID FLAGS RFC822.SIZE INTERNALDATE)')
        self.assertEqual(typ, 'OK')
        lines = [d.decode() for d in data]
        self.assertEqual(len(lines), 3)
        expected = imap_server.load_message(os.path.join(self.root, 'INBOX', '0002.eml'))
        self.assertRegex(lines[1], rf'UID 2 .*RFC822\.SIZE {len(expected)}')
        self.assertIn('INTERNALDATE "30-Sep-2026 10:42:00 +0200"', lines[1])
        typ, data = client.uid('FETCH', '2', '(UID BODY.PEEK[])')
        self.assertEqual(typ, 'OK')
        self.assertIn(b'UID 2', data[0][0])
        self.assertEqual(data[0][1], expected)
        self.assertTrue(expected.endswith(b'\r\n') and b'\r\n\r\nBody of message 2.' in expected)

    def test_a_new_file_gets_the_next_uid_under_the_same_uidvalidity(self):
        client = self.signed_in()
        client.select('INBOX', readonly=True)
        write(os.path.join(self.root, 'INBOX', '0000-late.eml'), MESSAGE.format(n=4))
        typ, data = client.select('INBOX', readonly=True)
        self.assertEqual(data, [b'4'])
        self.assertEqual(client.response('UIDVALIDITY')[1], [b'1'])
        self.assertEqual(client.response('UIDNEXT')[1], [b'5'])
        # The new file sorts first by name, but UIDs only grow.
        self.assertEqual(client.uid('SEARCH', 'UID', '4:*'), ('OK', [b'4']))

    def test_a_uidvalidity_file_renumbers_the_mailbox(self):
        with open(os.path.join(self.root, 'INBOX', '.uidvalidity'), 'w') as f:
            f.write('77\n')
        client = self.signed_in()
        client.select('INBOX', readonly=True)
        self.assertEqual(client.response('UIDVALIDITY')[1], [b'77'])

    def test_the_log_records_body_fetches_and_never_the_password(self):
        client = self.signed_in()
        client.select('INBOX', readonly=True)
        client.uid('FETCH', '1:2', '(UID BODY.PEEK[])')
        client.uid('FETCH', '1:3', '(UID FLAGS RFC822.SIZE INTERNALDATE)')
        client.logout()
        entries = self.log_entries()
        bodies = [e for e in entries if e.get('command') == 'UID FETCH' and e.get('body')]
        self.assertEqual(bodies, [
            {'command': 'UID FETCH', 'mailbox': 'INBOX', 'uids': [1, 2], 'body': True}])
        with open(self.log, encoding='utf-8') as f:
            self.assertNotIn(PASSWORD, f.read())

    def test_modified_utf7_encodes_like_rfc_3501(self):
        self.assertEqual(imap_server.encode_mutf7('Entwürfe'), 'Entw&APw-rfe')
        self.assertEqual(imap_server.encode_mutf7('Bills & Receipts'), 'Bills &- Receipts')
        self.assertEqual(imap_server.encode_mutf7('日本語'), '&ZeVnLIqe-')
        self.assertEqual(imap_server.encode_mutf7('~peter/mail/台北/日本語'),
                         '~peter/mail/&U,BTFw-/&ZeVnLIqe-')
        self.assertEqual(imap_server.encode_mutf7('INBOX'), 'INBOX')

    def test_messages_are_served_with_crlf_line_ends(self):
        path = os.path.join(self.root, 'INBOX', '0001.eml')
        served = imap_server.load_message(path)
        self.assertNotIn(b'\n', served.replace(b'\r\n', b''))
        with open(path, 'rb') as f:
            self.assertEqual(served, f.read().replace(b'\n', b'\r\n'))


@unittest.skipUnless(shutil.which('openssl'), 'openssl makes the test certificate')
class TestServerOverTls(ServerTest):
    tls = True

    def test_a_client_that_trusts_the_generated_certificate_signs_in(self):
        client = self.signed_in()
        typ, data = client.select('INBOX', readonly=True)
        self.assertEqual((typ, data), ('OK', [b'3']))

    def test_a_client_without_the_certificate_is_refused(self):
        with self.assertRaises(ssl.SSLError):
            imaplib.IMAP4_SSL('127.0.0.1', self.port, ssl_context=ssl.create_default_context())


if __name__ == '__main__':
    unittest.main()
