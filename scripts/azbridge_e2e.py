#!/usr/bin/env python3
"""The Azlin Bridge end to end: `azul-bridge` against the mock Azlin stack, driven by the mail
programs' and file managers' protocols through Python's own clients.

    cargo build --release -p azul-bridge
    python3 scripts/azbridge_e2e.py [--bin PATH] [--keep]

What it does, all on this computer and in one temporary folder (the user's ~/.azlin, keyring and
data are never read or written: HOME points into the folder, AZLIN_CONFIG is off):

  stack    the mock token server and S3 (scripts/azlin_mock_stack.py) in this process, and an
           SMTP sink (scripts/azmail_smtp_sink.py) standing in for the recipients' servers
  setup    `azul-bridge init` (the password printed once), `signup` (a drive of the bridge's own
           token family), `serve` on free ports with a 1-second IDLE poll; two messages put into
           mail/Inbox/ the way the customer's Email Worker would (PutObject, Azlin names)
  imap     imaplib: CAPABILITY, LOGIN (a wrong password refused), LIST (INBOX, the special-use
           folders), SELECT, FETCH (flags, size, envelope, a peeked header, BODY[] - which writes
           the drive's `seen` marker), UID STORE (the `flagged` marker), SEARCH, APPEND to Drafts
           (an Azlin name in mail/Drafts/), UID MOVE to Archive (the object moves, the markers
           stay), COPY + \\Deleted + EXPUNGE, IDLE seeing a message the Worker puts in
  smtp     smtplib: AUTH (a wrong password refused), a foreign sender refused, a mail to two
           recipients (one Bcc) through AzMail's sending path to the sink, its copy in the
           drive's mail/Sent/ (read), the program's own APPEND of it not filed twice
  webdav   http.client: OPTIONS, PROPFIND without and with Basic auth, PUT / GET / a range,
           PUT into a missing folder refused, COPY / MOVE, LOCK / UNLOCK (a PUT without the
           token refused), DELETE, a body with a DTD refused, a foreign Host refused, `..`
           refused, the sync's .azlin hidden
  pim      CalDAV / CardDAV over http.client on their own port: the well-known redirect, the
           principal's homes, a vCard 3.0 PUT kept byte for byte as contacts/<uid>.vcf, an
           event PUT under the program's own name read into AzCalendar's calendar/events/<id>.json
           and served back as iCalendar, both deleted
  doors    the ports answer nothing but 127.0.0.1 (a connection to this computer's network
           address is refused), an HTTP request on the IMAP port is hung up on

The mock S3 server stores no folder marker objects (keys ending in /), so MKCOL, deleting a
folder and IMAP CREATE / DELETE are left to the unit tests (they run on an in-memory bucket that
keeps markers, as S3 does).

The binary: --bin, else $AZUL_BRIDGE_BIN, else target/release/azul-bridge of this checkout.
Exit code 0 when every step passed.
"""

import argparse
import hashlib
import http.client
import imaplib
import json
import os
import re
import shutil
import smtplib
import socket
import subprocess
import sys
import tempfile
import threading
import time
from email.message import EmailMessage
from email.utils import make_msgid

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, HERE)

import azlin_mock_stack  # noqa: E402

ADDRESS = 'ada@example.org'
STAMP_NAME = re.compile(r'^\d{8}T\d{6}Z-[0-9a-f]{16}\.eml$')

# imaplib of older Pythons does not know these commands.
imaplib.Commands.setdefault('MOVE', ('SELECTED',))
imaplib.Commands.setdefault('IDLE', ('AUTH', 'SELECTED'))


class Failure(Exception):
    pass


def expect(cond, message):
    if not cond:
        raise Failure(message)


def find_binary(explicit):
    name = 'azul-bridge.exe' if os.name == 'nt' else 'azul-bridge'
    for candidate in (explicit, os.environ.get('AZUL_BRIDGE_BIN'),
                      os.path.join(REPO, 'target', 'release', name)):
        if candidate and os.path.isfile(candidate):
            return candidate
    raise Failure('no azul-bridge binary: cargo build --release -p azul-bridge (or --bin PATH)')


def object_name(data, stamp_secs):
    """`<YYYYMMDDTHHMMSSZ>-<first 16 hex of SHA-256>.eml` (AZLIN_MAIL.md section 2)."""
    return '%s-%s.eml' % (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime(stamp_secs)),
                          hashlib.sha256(data).hexdigest()[:16])


def mail(subject, body, sender='Ben <ben@example.net>', to=ADDRESS):
    msg = EmailMessage()
    msg['From'] = sender
    msg['To'] = to
    msg['Subject'] = subject
    msg['Date'] = 'Thu, 01 Oct 2026 08:00:00 +0000'
    msg['Message-ID'] = make_msgid(domain='example.net')
    msg.set_content(body)
    return msg.as_bytes().replace(b'\r\n', b'\n').replace(b'\n', b'\r\n')


class Drive:
    """The bridge's bucket as the mock S3 server keeps it: <root>/<bucket>/<key>."""

    def __init__(self, root, bucket):
        self.base = os.path.join(root, bucket)

    def keys(self, prefix=''):
        out = []
        for folder, dirs, files in os.walk(self.base):
            dirs[:] = [d for d in dirs if not d.startswith('.s3-server')]
            rel = os.path.relpath(folder, self.base)
            for name in files:
                key = name if rel == '.' else '/'.join(rel.split(os.sep) + [name])
                if key.startswith(prefix):
                    out.append(key)
        return sorted(out)

    def put(self, key, data):
        """What the Email Worker does: PutObject (here: the file the mock serves it as)."""
        path = os.path.join(self.base, *key.split('/'))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(data)

    def get(self, key):
        with open(os.path.join(self.base, *key.split('/')), 'rb') as f:
            return f.read()

    def has(self, key):
        return os.path.isfile(os.path.join(self.base, *key.split('/')))


class Bridge:
    def __init__(self, binary, work, token_url):
        self.binary = binary
        self.state = os.path.join(work, 'bridge-state')
        self.token_url = token_url
        home = os.path.join(work, 'home')
        os.makedirs(home, exist_ok=True)
        self.env = dict(os.environ)
        self.env.update({'HOME': home, 'USERPROFILE': home, 'AZLIN_CONFIG': 'off',
                         'AZLIN_TOKEN_URL': token_url})
        self.env.pop('AZUL_BRIDGE_HOME', None)
        self.process = None
        self.lines = []

    def run(self, *args):
        done = subprocess.run([self.binary, '--state-dir', self.state, *args], env=self.env,
                              capture_output=True, text=True, timeout=120)
        expect(done.returncode == 0, 'azul-bridge %s failed (%d): %s' % (
            args[0], done.returncode, done.stderr.strip()[-800:]))
        return done.stdout

    def serve(self):
        self.process = subprocess.Popen(
            [self.binary, '--state-dir', self.state, 'serve', '--imap-port', '0',
             '--smtp-port', '0', '--dav-port', '0', '--pim-port', '0', '--idle-poll', '1'],
            env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        ready = {}

        def read():
            for line in self.process.stdout:
                self.lines.append(line.rstrip('\n'))
                if line.startswith('AZUL_BRIDGE_READY'):
                    ready.update(dict(part.split('=') for part in line.split()[1:]))

        threading.Thread(target=read, daemon=True).start()
        deadline = time.time() + 60
        while not ready and time.time() < deadline:
            if self.process.poll() is not None:
                raise Failure('azul-bridge serve stopped: %s' % self.process.stderr.read()[-800:])
            time.sleep(0.1)
        expect(ready, 'azul-bridge serve did not get ready: %s' % self.lines)
        return int(ready['imap']), int(ready['smtp']), int(ready['dav']), int(ready['pim'])

    def stop(self):
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()


def start_sink(out_dir):
    process = subprocess.Popen([sys.executable, os.path.join(HERE, 'azmail_smtp_sink.py'), '0',
                                out_dir], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    line = process.stdout.readline()
    expect(line.startswith('AZMAIL_SINK_READY'), 'the SMTP sink did not start: %r' % line)
    return process, int(line.split()[1])


def wait_for(cond, seconds, message):
    deadline = time.time() + seconds
    while time.time() < deadline:
        if cond():
            return
        time.sleep(0.2)
    raise Failure(message)


# ---- the steps ----

def step_imap(port, password, drive, seeded):
    imap = imaplib.IMAP4('127.0.0.1', port, timeout=30)
    caps = imap.capabilities
    for cap in ('IMAP4REV1', 'IDLE', 'UIDPLUS', 'MOVE', 'SPECIAL-USE', 'AUTH=PLAIN'):
        expect(cap in caps, 'CAPABILITY lacks %s: %s' % (cap, caps))
    try:
        imap.login(ADDRESS, 'not-the-password')
        raise Failure('a wrong password was taken')
    except imaplib.IMAP4.error:
        pass
    imap = imaplib.IMAP4('127.0.0.1', port, timeout=30)
    typ, _ = imap.login(ADDRESS, password)
    expect(typ == 'OK', 'LOGIN failed')
    typ, boxes = imap.list()
    listing = [b.decode() for b in boxes]
    expect(any(l.endswith('"INBOX"') for l in listing), 'LIST has no INBOX: %s' % listing)
    for role, name in (('\\Sent', 'Sent'), ('\\Drafts', 'Drafts'), ('\\Junk', 'Spam'),
                       ('\\Trash', 'Trash'), ('\\Archive', 'Archive')):
        expect(any(role in l and l.endswith('"%s"' % name) for l in listing),
               'LIST has no %s %s: %s' % (role, name, listing))

    typ, data = imap.select('INBOX')
    expect(typ == 'OK' and data[0] == b'2', 'SELECT INBOX: %s %s' % (typ, data))
    typ, data = imap.fetch('1:*', '(UID FLAGS RFC822.SIZE ENVELOPE)')
    text = b' '.join(part if isinstance(part, bytes) else part[0] for part in data).decode()
    expect('UID 1' in text and 'UID 2' in text, 'FETCH: %s' % text)
    expect('RFC822.SIZE %d' % len(seeded[0][1]) in text, 'the size is the object\'s: %s' % text)
    expect('"Lunch on Thursday"' in text, 'ENVELOPE: %s' % text)
    typ, data = imap.fetch('1', '(BODY.PEEK[HEADER.FIELDS (SUBJECT)])')
    expect(b'Subject: Lunch on Thursday' in data[0][1], 'a peeked header: %r' % data)
    first_id = seeded[0][0][:-4]
    expect(not drive.has('mail/.state/%s/seen' % first_id), 'a PEEK wrote the seen marker')
    typ, data = imap.fetch('1', '(BODY[])')
    expect(data[0][1] == seeded[0][1], 'BODY[] is not the exact bytes')
    expect(drive.has('mail/.state/%s/seen' % first_id), 'BODY[] did not write the seen marker')

    second_id = seeded[1][0][:-4]
    typ, _ = imap.uid('STORE', '2', '+FLAGS', '(\\Flagged)')
    expect(typ == 'OK' and drive.has('mail/.state/%s/flagged' % second_id),
           'UID STORE \\Flagged did not write the flagged marker')
    typ, data = imap.search(None, 'UNSEEN')
    expect(data[0].split() == [b'2'], 'SEARCH UNSEEN: %s' % data)
    typ, data = imap.uid('SEARCH', 'SUBJECT', 'garden')
    expect(data[0].split() == [b'2'], 'UID SEARCH SUBJECT: %s' % data)

    draft = mail('Half written', 'More later.', sender=ADDRESS, to='ben@example.net')
    typ, data = imap.append('Drafts', '(\\Draft)', imaplib.Time2Internaldate(time.time()), draft)
    expect(typ == 'OK' and b'APPENDUID' in data[0], 'APPEND: %s %s' % (typ, data))
    drafts = drive.keys('mail/Drafts/')
    expect(len(drafts) == 1 and STAMP_NAME.match(drafts[0].rsplit('/', 1)[1]),
           'the draft is not one Azlin-named object: %s' % drafts)
    expect(drive.get(drafts[0]) == draft, 'the draft object is not the exact bytes')

    typ, data = imap.uid('MOVE', '1', 'Archive')
    expect(typ == 'OK', 'UID MOVE: %s %s' % (typ, data))
    expect(drive.has('mail/Archive/' + seeded[0][0]) and not drive.has('mail/Inbox/' + seeded[0][0]),
           'MOVE did not move the object: %s' % drive.keys('mail/'))
    expect(drive.has('mail/.state/%s/seen' % first_id), 'MOVE lost the seen marker')

    typ, _ = imap.copy('1', 'Trash')
    expect(typ == 'OK', 'COPY to Trash')
    imap.select('Trash')
    imap.store('1', '+FLAGS', '(\\Deleted)')
    typ, data = imap.expunge()
    expect(typ == 'OK' and data[0] == b'1', 'EXPUNGE: %s' % data)
    expect(not drive.keys('mail/Trash/'), 'EXPUNGE left the object: %s' % drive.keys('mail/Trash/'))
    expect(drive.has('mail/.state/%s/flagged' % second_id),
           'EXPUNGE of one copy dropped the marks the Inbox copy still has')

    # IDLE: the Worker puts a message in; the bridge tells about it.
    imap.select('INBOX')
    tag = imap._new_tag()
    imap.send(tag + b' IDLE\r\n')
    expect(imap.readline().startswith(b'+'), 'IDLE was not accepted')
    late = mail('Arrived while idling', 'Hello again.')
    drive.put('mail/Inbox/' + object_name(late, int(time.time())), late)
    imap.sock.settimeout(20)
    seen_exists = False
    deadline = time.time() + 20
    while time.time() < deadline and not seen_exists:
        line = imap.readline()
        seen_exists = b'EXISTS' in line
    expect(seen_exists, 'IDLE did not report the new message')
    imap.send(b'DONE\r\n')
    while True:
        line = imap.readline()
        if line.startswith(tag):
            expect(b' OK' in line, 'IDLE did not end with OK: %r' % line)
            break
    imap.logout()


def step_smtp(port, password, imap_port, drive, sink_dir):
    smtp = smtplib.SMTP('127.0.0.1', port, timeout=60)
    smtp.ehlo('e2e.local')
    expect(smtp.has_extn('auth'), 'EHLO offers no AUTH')
    try:
        smtp.login(ADDRESS, 'wrong')
        raise Failure('SMTP took a wrong password')
    except smtplib.SMTPAuthenticationError:
        pass
    smtp = smtplib.SMTP('127.0.0.1', port, timeout=60)
    smtp.ehlo('e2e.local')
    smtp.login(ADDRESS, password)
    try:
        smtp.sendmail('boss@example.com', ['ben@example.net'], mail('x', 'y', sender='boss@example.com'))
        raise Failure('SMTP sent as a foreign sender')
    except smtplib.SMTPSenderRefused:
        pass
    sent = mail('Through the bridge', 'Sent by a mail program.', sender=ADDRESS, to='ben@example.net')
    refused = smtp.sendmail(ADDRESS, ['ben@example.net', 'hidden@example.com'], sent)
    expect(not refused, 'some recipients refused: %s' % refused)
    smtp.quit()
    wait_for(lambda: any(n.endswith('.json') for n in os.listdir(sink_dir)), 30,
             'the sink got nothing')
    envelope = json.load(open(os.path.join(sink_dir, sorted(n for n in os.listdir(sink_dir)
                                                             if n.endswith('.json'))[0])))
    expect(sorted(envelope.get('rcpt_to', [])) == ['ben@example.net', 'hidden@example.com'],
           'the envelope: %s' % envelope)
    sent_keys = drive.keys('mail/Sent/')
    expect(len(sent_keys) == 1, 'the sent copy is not in mail/Sent/: %s' % drive.keys('mail/'))
    expect(drive.get(sent_keys[0]) == sent, 'the sent copy is not the message as it went out')
    sent_id = sent_keys[0].rsplit('/', 1)[1][:-4]
    expect(drive.has('mail/.state/%s/seen' % sent_id), 'the sent copy is not read')
    # The mail program files its own copy over IMAP: not a second one.
    imap = imaplib.IMAP4('127.0.0.1', imap_port, timeout=30)
    imap.login(ADDRESS, password)
    typ, data = imap.append('Sent', '(\\Seen)', imaplib.Time2Internaldate(time.time()), sent)
    expect(typ == 'OK', 'APPEND to Sent: %s %s' % (typ, data))
    expect(len(drive.keys('mail/Sent/')) == 1, 'the program\'s copy was filed twice')
    imap.logout()


def dav_request(port, method, path, body=b'', headers=None, password=None, host=None):
    conn = http.client.HTTPConnection('127.0.0.1', port, timeout=30)
    send = dict(headers or {})
    if password is not None:
        import base64
        token = base64.b64encode(('%s:%s' % (ADDRESS, password)).encode()).decode()
        send['Authorization'] = 'Basic ' + token
    if host is not None:
        conn.putrequest(method, path, skip_host=True, skip_accept_encoding=True)
        conn.putheader('Host', host)
        for name, value in send.items():
            conn.putheader(name, value)
        conn.putheader('Content-Length', str(len(body)))
        conn.endheaders(body)
    else:
        conn.request(method, path, body=body, headers=send)
    response = conn.getresponse()
    data = response.read()
    headers = {k.lower(): v for k, v in response.getheaders()}
    conn.close()
    return response.status, headers, data


def step_webdav(port, password, drive):
    status, headers, _ = dav_request(port, 'OPTIONS', '/')
    expect(status == 200 and headers.get('dav') == '1, 2', 'OPTIONS: %s %s' % (status, headers))
    status, headers, _ = dav_request(port, 'PROPFIND', '/', headers={'Depth': '1'})
    expect(status == 401 and 'basic' in headers.get('www-authenticate', '').lower(),
           'PROPFIND without signing in: %s' % status)
    status, _, data = dav_request(port, 'PROPFIND', '/', headers={'Depth': '1'}, password=password)
    expect(status == 207 and b'<D:href>/mail/</D:href>' in data, 'PROPFIND /: %s %s' % (status, data[:400]))
    expect(dav_request(port, 'PROPFIND', '/', headers={'Depth': '1'}, password='guess')[0] == 401,
           'a wrong password was taken')

    expect(dav_request(port, 'PUT', '/notes.txt', b'first draft', password=password)[0] == 201, 'PUT')
    expect(drive.get('notes.txt') == b'first draft', 'PUT did not reach the drive')
    status, _, data = dav_request(port, 'GET', '/notes.txt', password=password)
    expect((status, data) == (200, b'first draft'), 'GET: %s %r' % (status, data))
    status, headers, data = dav_request(port, 'GET', '/notes.txt', headers={'Range': 'bytes=6-10'},
                                        password=password)
    expect((status, data) == (206, b'draft') and headers.get('content-range') == 'bytes 6-10/11',
           'a range: %s %r %s' % (status, data, headers))

    expect(dav_request(port, 'PUT', '/docs/a.txt', b'a', password=password)[0] == 409,
           'PUT into a missing folder')
    drive.put('docs/readme.txt', b'read me')
    expect(dav_request(port, 'PUT', '/docs/a.txt', b'aaa', password=password)[0] == 201, 'PUT in docs/')
    copy = dav_request(port, 'COPY', '/docs/a.txt', password=password,
                       headers={'Destination': 'http://127.0.0.1:%d/docs/b.txt' % port})
    expect(copy[0] == 201 and drive.get('docs/b.txt') == b'aaa', 'COPY: %s' % copy[0])
    moved = dav_request(port, 'MOVE', '/docs/b.txt', password=password, headers={'Destination': '/c.txt'})
    expect(moved[0] == 201 and drive.has('c.txt') and not drive.has('docs/b.txt'), 'MOVE: %s' % moved[0])

    lockinfo = (b'<?xml version="1.0"?><D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/>'
                b'</D:lockscope><D:locktype><D:write/></D:locktype><D:owner>e2e</D:owner></D:lockinfo>')
    status, headers, _ = dav_request(port, 'LOCK', '/notes.txt', lockinfo, password=password,
                                     headers={'Timeout': 'Second-60', 'Depth': '0'})
    token = headers.get('lock-token', '')
    expect(status == 200 and token.startswith('<opaquelocktoken:'), 'LOCK: %s %s' % (status, token))
    expect(dav_request(port, 'PUT', '/notes.txt', b'x', password=password)[0] == 423,
           'a PUT without the lock token was taken')
    expect(dav_request(port, 'PUT', '/notes.txt', b'second draft', password=password,
                       headers={'If': '(%s)' % token})[0] == 204, 'a PUT with the lock token')
    expect(dav_request(port, 'UNLOCK', '/notes.txt', password=password,
                       headers={'Lock-Token': token})[0] == 204, 'UNLOCK')

    expect(dav_request(port, 'DELETE', '/c.txt', password=password)[0] == 204 and not drive.has('c.txt'),
           'DELETE')
    dtd = b'<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><D:propfind xmlns:D="DAV:">&e;</D:propfind>'
    expect(dav_request(port, 'PROPFIND', '/', dtd, password=password, headers={'Depth': '0'})[0] == 400,
           'a body with a DTD was parsed')
    expect(dav_request(port, 'GET', '/', password=password, host='attacker.example')[0] == 403,
           'a foreign Host was answered')
    expect(dav_request(port, 'GET', '/..%2f..%2fetc%2fpasswd', password=password)[0] == 400,
           '.. was not refused')
    drive.put('.azlin/index.json', b'{}')
    expect(dav_request(port, 'GET', '/.azlin/index.json', password=password)[0] == 404,
           'the sync\'s folder is reachable')


PIM_CARD = b'BEGIN:VCARD\r\nVERSION:3.0\r\nUID:ada-e2e\r\nFN:Ada E2E\r\nEND:VCARD\r\n'
PIM_EVENT = (b'BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//e2e//EN\r\nBEGIN:VEVENT\r\n'
             b'UID:e2e@example.org\r\nDTSTAMP:20261001T120000Z\r\nDTSTART:20261002T100000\r\n'
             b'DTEND:20261002T110000\r\nSUMMARY:E2E dentist\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n')


def step_pim(port, password, drive):
    status, headers, _ = dav_request(port, 'PROPFIND', '/.well-known/caldav', headers={'Depth': '0'},
                                     password=password)
    expect(status == 301 and headers.get('location') == '/', 'the well-known redirect: %s %s' % (status, headers))
    expect(dav_request(port, 'PROPFIND', '/principal/', headers={'Depth': '0'})[0] == 401,
           'PROPFIND without signing in')
    homes = (b'<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" '
             b'xmlns:CR="urn:ietf:params:xml:ns:carddav"><D:prop><C:calendar-home-set/>'
             b'<CR:addressbook-home-set/></D:prop></D:propfind>')
    status, _, data = dav_request(port, 'PROPFIND', '/principal/', homes, headers={'Depth': '0'}, password=password)
    expect(status == 207 and b'<D:href>/calendars/</D:href>' in data and b'<D:href>/addressbooks/</D:href>' in data,
           'the principal: %s %s' % (status, data[:400]))

    status, headers, _ = dav_request(port, 'PUT', '/addressbooks/contacts/ada-e2e.vcf', PIM_CARD,
                                     headers={'If-None-Match': '*', 'Content-Type': 'text/vcard'}, password=password)
    expect(status == 201 and 'etag' in headers, 'PUT a card: %s %s' % (status, headers))
    expect(drive.has('contacts/ada-e2e.vcf') and drive.get('contacts/ada-e2e.vcf') == PIM_CARD,
           'the card is not AzContacts\' file byte for byte: %s' % drive.keys('contacts/'))
    status, _, data = dav_request(port, 'GET', '/addressbooks/contacts/ada-e2e.vcf', password=password)
    expect(status == 200 and data == PIM_CARD, 'GET the card: %s' % status)

    status, headers, _ = dav_request(port, 'PUT', '/calendars/default/E2E-EVENT.ics', PIM_EVENT,
                                     headers={'If-None-Match': '*', 'Content-Type': 'text/calendar'}, password=password)
    expect(status == 201, 'PUT an event: %s' % status)
    files = drive.keys('calendar/events/')
    expect(len(files) == 1 and files[0].endswith('.json'), 'AzCalendar\'s event file: %s' % files)
    stored = json.loads(drive.get(files[0]))
    expect(stored.get('format') == 'azcalendar.event' and stored.get('title') == 'E2E dentist'
           and stored.get('uid') == 'e2e@example.org', 'the event file: %s' % stored)
    status, _, data = dav_request(port, 'PROPFIND', '/calendars/default/', headers={'Depth': '1'}, password=password)
    expect(status == 207 and b'<D:href>/calendars/default/E2E-EVENT.ics</D:href>' in data,
           'the calendar lists the program\'s own name: %s' % data[:600])
    status, _, data = dav_request(port, 'GET', '/calendars/default/E2E-EVENT.ics', password=password)
    expect(status == 200 and b'UID:e2e@example.org' in data and b'SUMMARY:E2E dentist' in data,
           'GET the event: %s %s' % (status, data[:400]))

    expect(dav_request(port, 'DELETE', '/calendars/default/E2E-EVENT.ics', password=password)[0] == 204
           and not drive.keys('calendar/events/'), 'DELETE the event')
    expect(dav_request(port, 'DELETE', '/addressbooks/contacts/ada-e2e.vcf', password=password)[0] == 204
           and not drive.has('contacts/ada-e2e.vcf'), 'DELETE the card')
    expect(dav_request(port, 'PROPFIND', '/principal/', headers={'Depth': '0'}, password=password,
                       host='attacker.example')[0] == 403, 'a foreign Host was answered')


def step_doors(ports):
    # A connection to this computer's network address (not 127.0.0.1) finds nobody.
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        probe.connect(('192.0.2.1', 9))
        address = probe.getsockname()[0]
    except OSError:
        address = None
    finally:
        probe.close()
    if address and not address.startswith('127.'):
        for port in ports:
            try:
                socket.create_connection((address, port), timeout=3).close()
                raise Failure('port %d answers on %s' % (port, address))
            except (ConnectionRefusedError, socket.timeout, OSError):
                pass
    else:
        print('[azbridge-e2e] doors: no network address to try (skipped that part)')
    # An HTTP request on the IMAP port: hung up on.
    s = socket.create_connection(('127.0.0.1', ports[0]), timeout=10)
    s.recv(1024)
    s.sendall(b'POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n')
    try:
        rest = s.recv(1024)
    except OSError:
        rest = b''
    expect(rest == b'', 'the IMAP port answered an HTTP request: %r' % rest)
    s.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--bin', help='the azul-bridge binary')
    parser.add_argument('--keep', action='store_true', help='keep the temporary folder')
    args = parser.parse_args()
    binary = find_binary(args.bin)
    work = tempfile.mkdtemp(prefix='azbridge-e2e-')
    s3_root = os.path.join(work, 's3')
    stack = azlin_mock_stack.start(s3_root)
    sink, sink_port = start_sink(os.path.join(work, 'sink'))
    bridge = Bridge(binary, work, stack.token_url)
    failed = []
    try:
        sending = os.path.join(work, 'sending.json')
        with open(sending, 'w') as f:
            json.dump({'route': {'kind': 'smtp', 'host': '127.0.0.1', 'port': sink_port}, 'tls': 'off'}, f)
        out = bridge.run('init', '--address', ADDRESS, '--sending', sending)
        found = [l.split(' ', 1)[1] for l in out.splitlines() if l.startswith('AZUL_BRIDGE_PASSWORD ')]
        expect(len(found) == 1, 'init printed no password')
        password = found[0].strip()
        again = bridge.run('init', '--address', ADDRESS, '--sending', sending)
        expect('AZUL_BRIDGE_PASSWORD' not in again, 'init printed the password a second time')
        bridge.run('signup', '--token-url', stack.token_url)
        record = json.load(open(os.path.join(bridge.state, 'azlin.json')))
        bucket = record['drives'][0]['bucket']
        drive = Drive(s3_root, bucket)
        state_text = ''.join(open(os.path.join(bridge.state, n), errors='replace').read()
                             for n in ('bridge.json', 'azlin.json', 'drives.json'))
        expect(password not in state_text, 'the password is in a settings file')
        seeded = []
        now = int(time.time()) - 3600
        for i, (subject, body) in enumerate((('Lunch on Thursday', 'See you at noon.'),
                                             ('The garden plan', 'Attached, more or less.'))):
            data = mail(subject, body)
            name = object_name(data, now + i * 60)
            drive.put('mail/Inbox/' + name, data)
            seeded.append((name, data))
        imap_port, smtp_port, dav_port, pim_port = bridge.serve()
        print('[azbridge-e2e] bridge on imap=%d smtp=%d dav=%d pim=%d' % (imap_port, smtp_port, dav_port, pim_port))
        for name, step in (('imap', lambda: step_imap(imap_port, password, drive, seeded)),
                           ('smtp', lambda: step_smtp(smtp_port, password, imap_port, drive,
                                                      os.path.join(work, 'sink'))),
                           ('webdav', lambda: step_webdav(dav_port, password, drive)),
                           ('pim', lambda: step_pim(pim_port, password, drive)),
                           ('doors', lambda: step_doors([imap_port, smtp_port, dav_port, pim_port]))):
            try:
                step()
                print('[azbridge-e2e] PASS %s' % name)
            except (Failure, OSError, imaplib.IMAP4.error, smtplib.SMTPException) as e:
                print('[azbridge-e2e] FAIL %s: %s' % (name, e))
                failed.append(name)
    except Failure as e:
        print('[azbridge-e2e] FAIL setup: %s' % e)
        failed.append('setup')
    finally:
        bridge.stop()
        sink.terminate()
        stack.stop()
        if args.keep:
            print('[azbridge-e2e] kept %s' % work)
        else:
            shutil.rmtree(work, ignore_errors=True)
    if failed:
        print('[azbridge-e2e] FAILED: %s' % ', '.join(failed))
        return 1
    print('[azbridge-e2e] all steps passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
