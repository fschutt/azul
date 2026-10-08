#!/usr/bin/env python3
"""AzMail signs in to an IMAP server and syncs its mail to files, twice, without fetching twice.

1. starts imap_server.py over a copy of sample_mail/ (plain TCP, or implicit TLS with --tls);
2. starts AzMail headless (AZ_BACKEND=headless, AZ_DEBUG=<port>) with an empty AZMAIL_DATA and
   the server's password in AZMAIL_TEST_PASSWORD (a headless run never touches the keyring);
3. through AzMail's debug server, Add Account (the empty window's message list offers it; the
   wizard of File > Info): types the address, Next,
   the IMAP host and port, ticks "Unencrypted connection" (without --tls), Next, Next (direct
   delivery), Finish;
4. waits for AZMAIL_SYNC_DONE and checks the files: account.json without the password, every
   message as mail/<folder>/<yyyy>/<mm>/<uid>.eml with the server's exact bytes, one index.jsonl
   line per message, state.json with UIDVALIDITY and the last UID, spam in mail/spam, and the
   password in no file at all;
5. checks the window: the folders, the messages, the plain text of a reply with its quotes,
   and the HTML part of a newsletter (its text, no image, the "download pictures" bar);
6. clicks "Send/Receive All Folders" again: nothing is fetched (AZMAIL_SYNC_DONE fetched=0, no
   body fetch in the server's log);
7. drops a new message into the server's INBOX and syncs once more: only that one is fetched;
8. pictures: drops a mail with a picture of its own (`cid:`), a web picture, a web background
   and a tracking pixel (both from a local web server) and syncs: its own picture shows at once
   (AZMAIL_PICTURE_SHOWN cid:...), nothing is fetched and the bar names what the pre-pass held
   back ("3 pictures"); "Download pictures" fetches the web picture alone - no background, no
   pixel, no cookie - and shows it.

Usage (from the azul repository, after building AzMail and libazul with the debug server):

    python3 examples/azul-mail/scripts/sync_e2e.py [--bin target/release/AzMail]
        [--debug-port 8769] [--timeout 90] [--tls] [--keep-logs]

Also read from the environment: AZMAIL_BIN. Logs and data go to a temporary folder that is
printed at the end (kept on failure, or always with --keep-logs).
"""
import argparse
import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import base64
import http.server
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
sys.path.insert(0, HERE)

import imap_server  # noqa: E402

USER = 'ada@example.org'
ACCOUNT_ID = 'ada@example.org'
# Server mailbox -> the local folder AzMail files it under (folders.rs).
FOLDERS = {
    'INBOX': 'inbox',
    'Spam': 'spam',
    'Sent': 'sent',
    'Entwürfe': 'Entwürfe',
    'Work': 'Work',
    'Work/Projects': 'Work.Projects',
}
NEW_MESSAGE = """Message-ID: <late-1@example.org>
Date: Wed, 30 Sep 2026 12:00:00 +0200
From: Ben Okafor <ben@example.org>
To: ada@example.org
Subject: One more thing about the bulbs

Bring gloves.
"""


PICTURE_MESSAGE = """Message-ID: <pictures-1@example.org>
Date: Wed, 30 Sep 2026 13:00:00 +0200
From: Garden Weekly <news@example.org>
To: ada@example.org
Subject: Pictures inside and outside
MIME-Version: 1.0
Content-Type: multipart/related; boundary="rel"

--rel
Content-Type: text/html; charset=utf-8

<html><body background="http://127.0.0.1:{port}/paper.png"><p>Our own logo:</p>
<img src="cid:own-logo@example.org" alt="Own logo" width="4" height="4">
<p>From the web:</p><img src="http://127.0.0.1:{port}/logo.png" alt="Web logo" width="4"
height="4"><img src="http://127.0.0.1:{port}/pixel.gif" width="1" height="1"></body></html>
--rel
Content-Type: image/png
Content-ID: <own-logo@example.org>
Content-Transfer-Encoding: base64

{png}
--rel--
"""


def png_bytes(width=4, height=4):
    """A small valid RGBA PNG (stdlib only)."""
    rows = b''.join(b'\x00' + b'\x2e\x7d\x32\xff' * width for _ in range(height))

    def chunk(kind, data):
        body = kind + data
        return struct.pack('>I', len(data)) + body + struct.pack('>I', zlib.crc32(body))
    header = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(rows))
            + chunk(b'IEND', b''))


class PictureServer:
    """A web server on 127.0.0.1 that serves one PNG at any path and records each request
    (its path and headers)."""

    def __init__(self):
        requests = self.requests = []
        picture = png_bytes()

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                requests.append((self.path, dict(self.headers)))
                self.send_response(200)
                self.send_header('Content-Type', 'image/png')
                self.send_header('Content-Length', str(len(picture)))
                self.send_header('Set-Cookie', 'tracked=1')
                self.end_headers()
                self.wfile.write(picture)

            def log_message(self, *args):
                pass

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def stop(self):
        self.server.shutdown()


def log(line):
    print(f'[sync-e2e] {line}', flush=True)


def main_repo():
    """The main checkout when this runs from a git worktree (for its target/)."""
    try:
        common = subprocess.run(
            ['git', '-C', REPO, 'rev-parse', '--path-format=absolute', '--git-common-dir'],
            capture_output=True, text=True, check=True).stdout.strip()
        return os.path.dirname(common)
    except Exception:
        return REPO


def find_binary(explicit):
    exe = 'AzMail.exe' if os.name == 'nt' else 'AzMail'
    candidates = [explicit, os.environ.get('AZMAIL_BIN')]
    for root in (REPO, main_repo()):
        for parts in (('release',), ('debug',), ('consumer', 'release'), ('consumer', 'debug')):
            candidates.append(os.path.join(root, 'target', *parts, exe))
    for c in candidates:
        if c and os.path.isfile(c):
            return os.path.abspath(c)
    raise SystemExit('the AzMail binary was not found (pass --bin); tried:\n  ' +
                     '\n  '.join(c for c in candidates if c))


class Failure(Exception):
    pass


class Run:
    def __init__(self, args):
        self.args = args
        self.deadline = time.time() + args.timeout
        self.tmp = tempfile.mkdtemp(prefix='azmail-sync-e2e-')
        self.server_root = os.path.join(self.tmp, 'server-mail')
        self.data = os.path.join(self.tmp, 'data')
        self.server_log = os.path.join(self.tmp, 'server.log')
        self.password = 'pw-' + secrets.token_urlsafe(18)
        self.children = []
        self.debug = args.debug_port

    # -- processes --

    def start(self, name, command, env):
        out = open(os.path.join(self.tmp, f'{name}.out'), 'w')
        err = open(os.path.join(self.tmp, f'{name}.err'), 'w')
        child = subprocess.Popen(command, env={**os.environ, **env}, stdout=out, stderr=err,
                                 stdin=subprocess.DEVNULL)
        self.children.append((name, child))
        return child

    def stop_all(self):
        for _, child in self.children:
            if child.poll() is None:
                child.terminate()
        for _, child in self.children:
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()

    def output(self, name, stream='out'):
        try:
            with open(os.path.join(self.tmp, f'{name}.{stream}'), encoding='utf-8',
                      errors='replace') as f:
                return f.read()
        except OSError:
            return ''

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            try:
                value = check()
                if value:
                    return value
            except Exception as e:  # the app may not answer yet
                last = e
            time.sleep(interval)
        raise Failure(f'timed out waiting for {what}' + (f' (last error: {last})' if last else ''))

    # -- the debug server --

    def op(self, op):
        body = json.dumps(op if isinstance(op, dict) else {'op': op}).encode()
        request = urllib.request.Request(f'http://127.0.0.1:{self.debug}/', data=body,
                                         method='POST')
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.loads(response.read().decode('utf-8'))

    def must(self, op):
        answer = self.op(op)
        if isinstance(answer, dict) and answer.get('status') == 'error':
            raise Failure(f'{op} failed: {json.dumps(answer)[:200]}')
        return answer

    def texts(self):
        out = []

        def walk(value):
            if isinstance(value, str):
                out.append(value)
            elif isinstance(value, list):
                for v in value:
                    walk(v)
            elif isinstance(value, dict):
                for v in value.values():
                    walk(v)
        walk(self.op('get_node_hierarchy'))
        return out

    def shows(self, text):
        return any(text in t for t in self.texts())

    def click(self, text):
        return self.must({'op': 'click', 'text': text})

    def type_into(self, selector, text):
        # Leaving a field can redraw the form (the address changes the placeholders): focus,
        # let a redraw settle, focus again, then type.
        self.must({'op': 'focus_node', 'selector': selector})
        time.sleep(0.3)
        self.must({'op': 'focus_node', 'selector': selector})
        time.sleep(0.1)
        self.must({'op': 'text_input', 'text': text})
        time.sleep(0.2)

    # -- AzMail's stdout --

    def syncs(self):
        """The AZMAIL_SYNC_DONE / AZMAIL_SYNC_FAILED lines so far."""
        return re.findall(r'^AZMAIL_SYNC_(DONE|FAILED) (.*)$', self.output('azmail'), re.M)

    def wait_sync(self, count):
        def done():
            lines = self.syncs()
            if len(lines) >= count:
                return lines[count - 1]
            # A form error (a sign-in refused) says why nothing syncs.
            for t in self.texts():
                if t.startswith('Sign-in failed') or t.startswith('Enter your'):
                    raise Failure(f'the form says: {t}')
            return None
        kind, rest = self.until(f'sync #{count} (AZMAIL_SYNC_DONE on stdout)', done)
        if kind != 'DONE':
            raise Failure(f'sync #{count} failed: {rest}')
        return dict(re.findall(r'(\w+)=(\d+)', rest))

    def body_fetches(self):
        """`(mailbox, uid)` of every body the server sent (the server's UTF-8 names)."""
        out = []
        try:
            with open(self.server_log, encoding='utf-8') as f:
                for line in f:
                    entry = json.loads(line)
                    if entry.get('command') == 'UID FETCH' and entry.get('body'):
                        out.extend((entry['mailbox'], uid) for uid in entry['uids'])
        except OSError:
            pass
        return out

    # -- the checks --

    def expected(self):
        """Every served message: `{(server mailbox, uid): path}`, UIDs in file-name order."""
        out = {}
        for mailbox in FOLDERS:
            folder = os.path.join(self.server_root, *mailbox.split('/'))
            names = sorted(n for n in os.listdir(folder) if n.endswith('.eml'))
            for uid, name in enumerate(names, 1):
                out[(mailbox, uid)] = os.path.join(folder, name)
        return out

    def mail_root(self):
        return os.path.join(self.data, ACCOUNT_ID)

    def check_files(self, expected):
        root = self.mail_root()
        account_file = os.path.join(root, 'account.json')
        with open(account_file, encoding='utf-8') as f:
            account = json.load(f)
        if account.get('format') != 'azmail.account' or account.get('email') != USER:
            raise Failure(f'account.json is {account}')
        if account['imap'] != {'host': '127.0.0.1', 'port': self.port}:
            raise Failure(f'account.json names the server {account["imap"]}')
        if account.get('security') != ('tls' if self.args.tls else 'plain'):
            raise Failure(f'account.json says security {account.get("security")}')
        for mailbox, folder in FOLDERS.items():
            base = os.path.join(root, 'mail', folder)
            with open(os.path.join(base, 'index.jsonl'), encoding='utf-8') as f:
                index = [json.loads(line) for line in f if line.strip()]
            with open(os.path.join(base, 'state.json'), encoding='utf-8') as f:
                state = json.load(f)
            uids = sorted(uid for (m, uid) in expected if m == mailbox)
            if [e['uid'] for e in index] != uids:
                raise Failure(f'{folder}/index.jsonl has UIDs {[e["uid"] for e in index]}, '
                              f'not {uids}')
            if state.get('uidvalidity') != 1 or state.get('last_uid') != (uids[-1] if uids else 0):
                raise Failure(f'{folder}/state.json is {state}')
            if state.get('server_name') != imap_server.encode_mutf7(mailbox):
                raise Failure(f'{folder}/state.json names {state.get("server_name")}')
            for entry in index:
                path = os.path.join(root, *entry['path'].split('/'))
                if not re.fullmatch(rf'mail/{re.escape(folder)}/\d{{4}}/\d{{2}}/{entry["uid"]}\.eml',
                                    entry['path']):
                    raise Failure(f'{entry["path"]} is not mail/{folder}/<yyyy>/<mm>/<uid>.eml')
                with open(path, 'rb') as f:
                    got = f.read()
                want = imap_server.load_message(expected[(mailbox, entry['uid'])])
                if got != want:
                    raise Failure(f'{entry["path"]} is not the served message byte for byte')
                if entry['size'] != len(want) or not entry['subject'] or not entry['date']:
                    raise Failure(f'{folder}/index.jsonl line is incomplete: {entry}')
        # The spam folder holds the spam.
        with open(os.path.join(root, 'mail', 'spam', 'index.jsonl'), encoding='utf-8') as f:
            spam = [json.loads(line) for line in f if line.strip()]
        if [e['subject'] for e in spam] != ['Urgent: verify your account']:
            raise Failure(f'mail/spam holds {spam}')
        # The Latin-1 subject was decoded.
        with open(os.path.join(root, 'mail', 'inbox', 'index.jsonl'), encoding='utf-8') as f:
            subjects = [json.loads(line)['subject'] for line in f if line.strip()]
        if 'Grüße aus München' not in subjects:
            raise Failure(f'the inbox subjects are {subjects}')
        # The password is in no file AzMail wrote.
        secret = self.password.encode()
        for dirpath, _, files in os.walk(self.data):
            for name in files:
                with open(os.path.join(dirpath, name), 'rb') as f:
                    if secret in f.read():
                        raise Failure(f'the password is in {os.path.join(dirpath, name)}')
        if secret in (self.output('azmail') + self.output('azmail', 'err')).encode():
            raise Failure('the password is in AzMail\'s output')

    def run(self):
        shutil.copytree(os.path.join(HERE, 'sample_mail'), self.server_root)
        os.makedirs(self.data)
        binary = find_binary(self.args.bin)
        log(f'AzMail: {binary}')
        log(f'logs and data: {self.tmp}')

        server_cmd = [sys.executable, os.path.join(HERE, 'imap_server.py'), '--root',
                      self.server_root, '--port', '0', '--user', USER, '--log', self.server_log]
        if self.args.tls:
            server_cmd.append('--tls')
        self.start('server', server_cmd, {'AZMAIL_TEST_PASSWORD': self.password})
        self.port = int(self.until('the IMAP server', lambda: (re.search(
            r'^IMAP_SERVER_PORT (\d+)$', self.output('server'), re.M) or [None, None])[1]))
        env = {
            'AZ_BACKEND': 'headless',
            'AZ_DEBUG': str(self.debug),
            'AZMAIL_DATA': self.data,
            'AZMAIL_TEST_PASSWORD': self.password,
            # The kit's data root and the shared Azlin config stay out of the run: never the
            # user's own.
            'AZLIN_DATA': os.path.join(self.tmp, 'azlin-data'),
            'AZLIN_CONFIG': 'off',
        }
        if self.args.tls:
            ca = self.until('the server certificate', lambda: (re.search(
                r'^IMAP_SERVER_CA (.+)$', self.output('server'), re.M) or [None, None])[1])
            env['AZMAIL_TEST_CA'] = ca
        log(f'IMAP server on 127.0.0.1:{self.port}' + (' (TLS)' if self.args.tls else ''))

        self.start('azmail', [binary], env)
        # No account: the real window, its message list saying so and offering Add Account
        # (the wizard of File > Info) - no wizard in front of the window.
        self.until('the empty mail window', lambda: self.shows('No account yet'))
        self.click('Add Account')
        self.until('the Add Account wizard', lambda: self.shows('Your account'))
        time.sleep(0.5)
        self.type_into('#__azmail_acct_email', USER)
        self.click('Next >')
        self.until('the incoming server page', lambda: self.shows('Incoming mail server'))
        self.type_into('#__azmail_acct_imap_host', '127.0.0.1')
        self.type_into('#__azmail_acct_imap_port', str(self.port))
        if not self.args.tls:
            self.click('Unencrypted connection')
            time.sleep(0.3)
        self.click('Next >')
        self.until('the sending page', lambda: self.shows('Send mail:'))
        self.click('Next >')
        self.until('the last page', lambda: self.shows('Finish adds the account'))
        self.click('Finish')

        first = self.wait_sync(1)
        expected = self.expected()
        log(f'first sync: {first}')
        if int(first.get('fetched', -1)) != len(expected):
            raise Failure(f'the first sync fetched {first.get("fetched")}, not {len(expected)}')
        self.check_files(expected)
        fetched = self.body_fetches()
        # The server logs its own (UTF-8) mailbox names.
        if sorted(fetched) != sorted(expected):
            raise Failure(f'the server sent the bodies {fetched}')
        log(f'{len(expected)} messages in {len(FOLDERS)} folders, byte for byte, spam in mail/spam')

        # The window: folders, messages, a reply's quotes, a newsletter's HTML part.
        for text in ('Inbox', 'Junk E-mail', 'Garden Weekly: bulbs, frost and a sale'):
            self.until(f'the window to show "{text}"', lambda: self.shows(text))
        self.click('Re: Garden plan for October')
        self.until('the reply\'s body', lambda: self.shows('I can bring the tulip bulbs'))
        self.until('its quoted lines', lambda: self.shows('Last year it came early.'))
        self.click('Garden Weekly: bulbs, frost and a sale')
        self.until('the newsletter\'s HTML part', lambda: self.shows('See the sale'))
        self.until('the pictures bar', lambda: self.shows('Click here to download pictures'))
        if self.shows('document.write') or self.shows('Sign in'):
            raise Failure('the HTML view shows a script or a form')
        log('the window lists the folders and shows the reply and the newsletter\'s HTML')

        # Again: nothing is fetched twice.
        before = len(self.body_fetches())
        self.click('Send/Receive All Folders')
        second = self.wait_sync(2)
        log(f'second sync: {second}')
        if int(second.get('fetched', -1)) != 0 or len(self.body_fetches()) != before:
            raise Failure(f'the second sync fetched {second}, server log '
                          f'{self.body_fetches()[before:]}')

        # New mail: only it is fetched.
        imap_server_inbox = os.path.join(self.server_root, 'INBOX')
        with open(os.path.join(imap_server_inbox, '0005-late.eml'), 'w', newline='\n') as f:
            f.write(NEW_MESSAGE)
        self.click('Send/Receive All Folders')
        third = self.wait_sync(3)
        log(f'third sync: {third}')
        new = self.body_fetches()[before:]
        if int(third.get('fetched', -1)) != 1 or new != [('INBOX', 5)]:
            raise Failure(f'the third sync fetched {third}, server log {new}')
        with open(os.path.join(self.mail_root(), 'mail', 'inbox', 'index.jsonl'),
                  encoding='utf-8') as f:
            inbox = [json.loads(line) for line in f if line.strip()]
        if [e['uid'] for e in inbox] != [1, 2, 3, 4, 5]:
            raise Failure(f'the inbox index has {[e["uid"] for e in inbox]}')
        log('a second sync fetched nothing; a new message was fetched alone')
        self.pictures(imap_server_inbox)

    def pictures(self, inbox):
        """Step 8: the mail's own picture at once, its web picture after the click only."""
        web = PictureServer()
        try:
            png = base64.encodebytes(png_bytes()).decode('ascii').strip()
            with open(os.path.join(inbox, '0006-pictures.eml'), 'w', newline='\n') as f:
                f.write(PICTURE_MESSAGE.format(port=web.port, png=png))
            self.click('Send/Receive All Folders')
            self.wait_sync(4)
            self.click('Pictures inside and outside')
            self.until('the pictures mail', lambda: self.shows('Our own logo:'))
            self.until('its own picture', lambda: 'AZMAIL_PICTURE_SHOWN cid:' in self.output('azmail'))
            self.until('the pictures bar', lambda: self.shows('Click here to download pictures'))
            if not self.shows('prevented automatic download of 3 pictures'):
                raise Failure('the bar does not name the 3 web pictures the pre-pass found')
            if web.requests:
                raise Failure(f'fetched before the click: {[r[0] for r in web.requests]}')
            self.click('Download pictures')
            shown = f'AZMAIL_PICTURE_SHOWN http://127.0.0.1:{web.port}/logo.png'
            self.until('the web picture', lambda: shown in self.output('azmail'))
            if 'AZMAIL_PICTURES_FETCH 1' not in self.output('azmail'):
                raise Failure('the fetch list is not the one shown web picture')
            time.sleep(1.0)
            paths = [path for path, _ in web.requests]
            if paths != ['/logo.png']:
                raise Failure(f'fetched {paths}: only the shown web picture may be')
            headers = {k.lower(): v for k, v in web.requests[0][1].items()}
            if 'cookie' in headers or headers.get('user-agent') != 'AzMail':
                raise Failure(f'the picture request sent {headers}')
            log('own picture at once; the web picture alone after "Download pictures"')
        finally:
            web.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    parser.add_argument('--bin')
    parser.add_argument('--debug-port', type=int, default=8769)
    parser.add_argument('--timeout', type=float, default=90)
    parser.add_argument('--tls', action='store_true', help='implicit TLS with a test certificate')
    parser.add_argument('--keep-logs', action='store_true')
    args = parser.parse_args()
    run = Run(args)
    passed = False
    try:
        run.run()
        passed = True
        log('PASS: AzMail synced every folder to files, and a re-sync fetched nothing twice')
    except (Failure, OSError, ValueError, KeyError) as e:
        log(f'FAIL: {e}')
        for name, _ in run.children:
            print(f'\n----- {name} stdout (tail) -----\n{run.output(name)[-3000:]}')
            print(f'----- {name} stderr (tail) -----\n{run.output(name, "err")[-3000:]}')
    finally:
        run.stop_all()
        if passed and not args.keep_logs:
            shutil.rmtree(run.tmp, ignore_errors=True)
        else:
            log(f'logs kept in {run.tmp}')
    sys.exit(0 if passed else 1)


if __name__ == '__main__':
    main()
