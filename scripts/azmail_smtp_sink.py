#!/usr/bin/env python3
"""A local SMTP server that stores every message it receives as an .eml file (stdlib only).

    python3 scripts/azmail_smtp_sink.py <port> <out_dir> [options]

It speaks EHLO / HELO / MAIL / RCPT / DATA / RSET / NOOP / QUIT (RFC 5321, with the dot-stuffing
of DATA undone), STARTTLS when it has a certificate, TLS from the first byte with
--implicit-tls, and AUTH PLAIN / LOGIN / XOAUTH2 with --auth (a submission server: offered only
on an encrypted connection when it has a certificate, as Gmail and Outlook do, and required
before MAIL). For each message it writes `<out_dir>/<nnnn>.eml` (the bytes as a server stores
them) and `<out_dir>/<nnnn>.json` (the envelope: helo, mail_from, rcpt_to, tls,
bare_line_ends, auth_user, auth_mechanism - never the secret). `smtpd` left Python in 3.12, so this is
a small socketserver one. Port 0 picks a free port. When it listens it prints
`AZMAIL_SINK_READY <port>` (and `AZMAIL_SINK_CERT <pem>` with --tls-selfsigned), flushed, so a
script can wait for it.

Options:
  --cert PEM --key PEM      offer STARTTLS with this certificate
  --tls-selfsigned          make a certificate for localhost / 127.0.0.1 in out_dir (needs the
                            openssl command) and offer STARTTLS with it
  --reject ADDR=REPLY       answer `RCPT TO:<ADDR>` with REPLY (e.g. "550 5.1.1 no such user");
                            repeatable
  --data-reply REPLY        the reply to the end of DATA (default "250 2.0.0 queued as <n>")
  --greeting REPLY          the greeting (default "220 azmail-sink ESMTP"); a 5xx greeting
                            closes every connection right after it
  --max-messages N          exit after N messages (default: run until killed)
  --auth USER=SECRET        a submission server: take this sign-in (password or OAuth token);
                            repeatable; MAIL needs a sign-in first
  --auth-mechs "M ..."      the AUTH mechanisms offered (default "PLAIN LOGIN XOAUTH2")
  --implicit-tls            TLS from the first byte (port 465's way) instead of STARTTLS

Used by scripts/azmail_send_test.py (AzMail's send path) and by MAIL2's E2E (route = SMTP
127.0.0.1:<port>).
"""
import argparse
import base64
import binascii
import json
import os
import socketserver
import ssl
import sys
import threading
import time

MAX_LINE = 1000 * 1000


class Sink:
    def __init__(self, out_dir, tls_context, rejects, data_reply, greeting, max_messages,
                 accounts=None, mechanisms=(), implicit_tls=False):
        self.out_dir = out_dir
        self.tls_context = tls_context
        # user -> secret; empty: no AUTH at all (a relay or an MX).
        self.accounts = accounts or {}
        self.mechanisms = list(mechanisms)
        self.implicit_tls = implicit_tls
        self.rejects = rejects
        self.data_reply = data_reply
        self.greeting = greeting
        self.max_messages = max_messages
        self.count = 0
        self.lock = threading.Lock()
        self.done = threading.Event()

    def store(self, message, envelope):
        with self.lock:
            self.count += 1
            n = self.count
            stem = os.path.join(self.out_dir, f'{n:04d}')
            with open(stem + '.eml.tmp', 'wb') as f:
                f.write(message)
            os.replace(stem + '.eml.tmp', stem + '.eml')
            with open(stem + '.json', 'w', encoding='utf-8') as f:
                json.dump(envelope, f, indent=2)
                f.write('\n')
            if self.max_messages and n >= self.max_messages:
                self.done.set()
            return n


class Handler(socketserver.StreamRequestHandler):
    timeout = 60

    def setup(self):
        super().setup()
        self.tls = False
        self.auth_user = None
        self.auth_mechanism = None

    def start_tls(self):
        """Wraps the connection in TLS (server side); False when the handshake failed."""
        sink = self.server.sink
        try:
            tls_socket = sink.tls_context.wrap_socket(self.request, server_side=True)
        except (ssl.SSLError, OSError) as e:
            print(f'azmail-sink: TLS handshake failed: {e}', file=sys.stderr, flush=True)
            return False
        self.request = tls_socket
        self.connection = tls_socket
        self.rfile = tls_socket.makefile('rb')
        self.wfile = tls_socket.makefile('wb')
        self.tls = True
        return True

    def offers_auth(self):
        sink = self.server.sink
        return bool(sink.accounts) and (self.tls or sink.tls_context is None)

    def authenticate(self, arg):
        """AUTH <mechanism> [initial response]: PLAIN, LOGIN or XOAUTH2 (RFC 4954, 7628)."""
        sink = self.server.sink
        words = arg.split()
        mechanism = words[0].upper() if words else ''
        initial = words[1] if len(words) > 1 else None
        if not self.offers_auth() or mechanism not in sink.mechanisms:
            self.say('504 5.5.4 mechanism not offered')
            return
        if self.auth_user is not None:
            self.say('503 5.5.1 already signed in')
            return

        def read_response():
            raw = self.readline()
            return raw.decode('ascii', 'replace').strip() if raw else None

        def decode(text):
            try:
                return base64.b64decode(text or '', validate=True).decode('utf-8', 'replace')
            except (binascii.Error, ValueError):
                return None

        user = secret = None
        if mechanism == 'PLAIN':
            if initial is None:
                self.say('334 ')
                initial = read_response()
            decoded = decode(initial)
            if decoded is not None and decoded.count('\0') == 2:
                _, user, secret = decoded.split('\0')
        elif mechanism == 'LOGIN':
            # The user name may come along (`AUTH LOGIN <user>`, as Postfix and Dovecot take).
            if initial is None:
                self.say('334 ' + base64.b64encode(b'Username:').decode())
                initial = read_response()
            user = decode(initial)
            self.say('334 ' + base64.b64encode(b'Password:').decode())
            secret = decode(read_response())
        elif mechanism == 'XOAUTH2':
            if initial is None:
                self.say('334 ')
                initial = read_response()
            decoded = decode(initial) or ''
            fields = dict(f.split('=', 1) for f in decoded.split('\x01') if '=' in f)
            user = fields.get('user')
            auth = fields.get('auth', '')
            secret = auth[len('Bearer '):] if auth.startswith('Bearer ') else None
        if user is not None and secret is not None and sink.accounts.get(user) == secret:
            self.auth_user, self.auth_mechanism = user, mechanism
            self.say('235 2.7.0 Authentication successful')
            return
        if mechanism == 'XOAUTH2':
            # Google's way: the error report as a challenge, the refusal after the empty line.
            report = base64.b64encode(b'{"status":"401","schemes":"bearer"}').decode()
            self.say('334 ' + report)
            read_response()
        self.say('535 5.7.8 Username and Password not accepted')

    def say(self, line):
        self.wfile.write(line.encode('utf-8', 'replace') + b'\r\n')
        self.wfile.flush()

    def readline(self):
        line = self.rfile.readline(MAX_LINE)
        return line

    def handle(self):
        sink = self.server.sink
        if sink.implicit_tls and not self.start_tls():
            return
        self.say(sink.greeting)
        if not sink.greeting.startswith('2'):
            return
        helo = ''
        mail_from = None
        rcpt_to = []
        while True:
            raw = self.readline()
            if not raw:
                return
            line = raw.decode('utf-8', 'replace').rstrip('\r\n')
            verb = line.split(' ', 1)[0].upper()
            arg = line[len(verb):].strip()
            if verb == 'EHLO':
                helo = arg
                mail_from, rcpt_to = None, []
                caps = ['azmail-sink', '8BITMIME', 'SIZE 52428800']
                if sink.tls_context is not None and not self.tls:
                    caps.append('STARTTLS')
                if self.offers_auth():
                    caps.append('AUTH ' + ' '.join(sink.mechanisms))
                for cap in caps[:-1]:
                    self.say('250-' + cap)
                self.say('250 ' + caps[-1])
            elif verb == 'HELO':
                helo = arg
                mail_from, rcpt_to = None, []
                self.say('250 azmail-sink')
            elif verb == 'STARTTLS':
                if sink.tls_context is None or self.tls:
                    self.say('502 5.5.1 no TLS here')
                    continue
                self.say('220 2.0.0 ready to start TLS')
                if not self.start_tls():
                    return
                helo, mail_from, rcpt_to = '', None, []
            elif verb == 'AUTH':
                self.authenticate(arg)
            elif verb == 'MAIL':
                if sink.accounts and self.auth_user is None:
                    self.say('530 5.7.0 Authentication required')
                    continue
                if not arg.upper().startswith('FROM:'):
                    self.say('501 5.5.4 MAIL FROM:<address>')
                    continue
                mail_from = address_of(arg[5:])
                rcpt_to = []
                self.say('250 2.1.0 OK')
            elif verb == 'RCPT':
                if mail_from is None:
                    self.say('503 5.5.1 MAIL first')
                    continue
                if not arg.upper().startswith('TO:'):
                    self.say('501 5.5.4 RCPT TO:<address>')
                    continue
                address = address_of(arg[3:])
                reply = sink.rejects.get(address.lower())
                if reply:
                    self.say(reply)
                else:
                    rcpt_to.append(address)
                    self.say('250 2.1.5 OK')
            elif verb == 'DATA':
                if not rcpt_to:
                    self.say('503 5.5.1 RCPT first')
                    continue
                self.say('354 end with <CRLF>.<CRLF>')
                message, bare = self.read_data()
                if message is None:
                    return
                envelope = {
                    'helo': helo,
                    'mail_from': mail_from,
                    'rcpt_to': rcpt_to,
                    'tls': self.tls,
                    'bare_line_ends': bare,
                    'auth_user': self.auth_user,
                    'auth_mechanism': self.auth_mechanism,
                    'received_at': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                }
                if sink.data_reply:
                    self.say(sink.data_reply)
                    if sink.data_reply.startswith('2'):
                        sink.store(message, envelope)
                else:
                    n = sink.store(message, envelope)
                    self.say(f'250 2.0.0 queued as {n:04d}')
                mail_from, rcpt_to = None, []
            elif verb == 'RSET':
                mail_from, rcpt_to = None, []
                self.say('250 2.0.0 OK')
            elif verb == 'NOOP':
                self.say('250 2.0.0 OK')
            elif verb == 'VRFY':
                self.say('252 2.0.0 cannot verify')
            elif verb == 'QUIT':
                self.say('221 2.0.0 bye')
                return
            else:
                self.say('500 5.5.2 unrecognised command')

    def read_data(self):
        """The message up to the `.` line, one leading dot of each line removed (RFC 5321
        4.5.2). Returns (bytes, whether a line ended without CRLF)."""
        parts = []
        bare = False
        while True:
            line = self.rfile.readline(MAX_LINE)
            if not line:
                return None, bare
            if line in (b'.\r\n', b'.\n'):
                return b''.join(parts), bare
            if not line.endswith(b'\r\n') or b'\r' in line[:-2]:
                bare = True
            if line.startswith(b'.'):
                line = line[1:]
            parts.append(line)


def address_of(text):
    text = text.strip()
    if '<' in text and '>' in text:
        return text[text.index('<') + 1:text.index('>')]
    return text.split(' ', 1)[0]


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def self_signed(out_dir):
    """The IMAP test server's self-signed certificate recipe (one recipe for both test servers):
    P-256, 127.0.0.1 and localhost, its own trust anchor for the client (`--ca`)."""
    here = os.path.dirname(os.path.abspath(__file__))
    sys.path.insert(0, os.path.join(here, '..', 'examples', 'azul-mail', 'scripts'))
    from imap_server import make_self_signed_cert  # noqa: E402
    return make_self_signed_cert(out_dir)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('port', type=int)
    ap.add_argument('out_dir')
    ap.add_argument('--cert')
    ap.add_argument('--key')
    ap.add_argument('--tls-selfsigned', action='store_true')
    ap.add_argument('--reject', action='append', default=[], metavar='ADDR=REPLY')
    ap.add_argument('--data-reply')
    ap.add_argument('--greeting', default='220 azmail-sink ESMTP')
    ap.add_argument('--max-messages', type=int, default=0)
    ap.add_argument('--auth', action='append', default=[], metavar='USER=SECRET')
    ap.add_argument('--auth-mechs', default='PLAIN LOGIN XOAUTH2')
    ap.add_argument('--implicit-tls', action='store_true')
    args = ap.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    tls_context = None
    cert, key = args.cert, args.key
    if args.tls_selfsigned:
        cert, key = self_signed(args.out_dir)
    if cert and key:
        tls_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls_context.load_cert_chain(cert, key)
    rejects = {}
    for spec in args.reject:
        address, _, reply = spec.partition('=')
        rejects[address.strip().lower()] = reply.strip() or '550 5.1.1 rejected'

    accounts = {}
    for spec in args.auth:
        user, _, secret = spec.partition('=')
        accounts[user] = secret
    if args.implicit_tls and tls_context is None:
        ap.error('--implicit-tls needs --cert/--key or --tls-selfsigned')
    sink = Sink(args.out_dir, tls_context, rejects, args.data_reply, args.greeting,
                args.max_messages, accounts, args.auth_mechs.upper().split(),
                args.implicit_tls)
    server = Server(('127.0.0.1', args.port), Handler)
    server.sink = sink
    port = server.server_address[1]
    print(f'AZMAIL_SINK_READY {port}', flush=True)
    if tls_context is not None and args.tls_selfsigned:
        print(f'AZMAIL_SINK_CERT {cert}', flush=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        while not sink.done.is_set():
            sink.done.wait(0.5)
        time.sleep(0.2)  # let the client read the last reply
    except KeyboardInterrupt:
        pass
    server.shutdown()


if __name__ == '__main__':
    main()
