#!/usr/bin/env python3
"""AzMail's send path end to end: `azmail-send` (examples/azul-mail/src/bin/azmail_send.rs, the
code of `azmail::send::send_mail`) against the local SMTP sink (scripts/azmail_smtp_sink.py).

Cases (each with a fresh AzMail folder and its own sink):
  smtp          SMTP route to the sink: headers (From, To, Cc, no Bcc, RFC 2047 Subject,
                MIME-Version, Message-ID, In-Reply-To), the envelope (Bcc in RCPT), a dot-led
                line, the attachment; the mail in mail/sent/ (index.jsonl + the same bytes);
                the outbox empty
  rejected      one recipient answered 550 5.1.1: "failed", the others got it, filed in Sent,
                the policy list unchanged (an SMTP relay's answer says nothing about a domain)
  queued        the sink answers 451 to the data: "queued", the outbox holds .eml + .json;
                --retry --force against a healthy sink: "sent", the outbox empty
  direct        direct delivery to `ann@localhost` (the MX of localhost is 127.0.0.1) on the
                sink's port (--direct-port): "sent"
  policy        direct delivery to a gmail.com address: "queued" without any connection, and
                send_policy.json ships gmail.com as "relay"
  starttls      (needs openssl) the sink offers STARTTLS with a self-signed certificate;
                --tls required --ca <cert>: "sent" over TLS
  dkim          (needs openssl) --dkim-domain/--dkim-key: a DKIM-Signature with d= and s=, bh=
                the SHA-256 of the relaxed body, and the whole signature verified by this
                script's own RFC 6376 verifier (relaxed canonicalization here, RSA-SHA256 by the
                openssl command; dkimpy too when it is installed)
  dkim-generated (needs openssl) --dkim-generate: AzMail makes the key (as the Sending page
                does) and prints the DNS record; p= is the key file's SubjectPublicKeyInfo (by
                openssl), the key file is 0600, and the mail verifies against that record
  port25        direct delivery to a closed local port with the port-25 probe pointed at it:
                queued, send_policy.json records port25.open = false; a retry within the hour
                knocks nowhere; --ignore-policy sends it to a sink
  submission    (needs openssl) the optional signed-in route (lettre): the account's own
                outgoing server from account.json, STARTTLS with a self-signed certificate,
                AUTH PLAIN with the user name and the secret (from the environment); the
                sink saw TLS and the sign-in; the mail in Sent; the secret in no output or file
  submission-implicit (needs openssl) TLS from the first byte (--tls implicit, port 465's way)
                and AUTH LOGIN (the only mechanism offered)
  submission-xoauth2 (needs openssl) an OAuth account: XOAUTH2 with the token
  submission-refused (needs openssl) a wrong password: queued with the server's 535, no
                attempt counted, nothing stored; the retry with the right one: sent

Usage (from the azul repository, after `cargo build --release -p AzMail --bin azmail-send`):

    python3 scripts/azmail_send_test.py [--bin target/release/azmail-send] [--case smtp ...]
        [--keep] [--runner "<run_capped.sh> --cap-mb 300 --seconds 60 --log /tmp/send.log --"]

Also read from the environment: AZMAIL_SEND_BIN. The binary links libazul dynamically:
DYLD_LIBRARY_PATH / LD_LIBRARY_PATH get target/release and target/azul-lib. Exit 0 when every
case passed.
"""
import argparse
import base64
import email
import email.policy
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..'))
SINK = os.path.join(HERE, 'azmail_smtp_sink.py')
ACCOUNT = 'ada@example.org'


class Failure(Exception):
    pass


def check(condition, what):
    if not condition:
        raise Failure(what)


def find_bin(arg):
    candidates = [arg, os.environ.get('AZMAIL_SEND_BIN'),
                  os.path.join(REPO, 'target', 'release', 'azmail-send'),
                  os.path.join(REPO, 'target', 'consumer', 'release', 'azmail-send')]
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    sys.exit('azmail-send not found: build it with '
             '`cargo build --release -p AzMail --bin azmail-send` or pass --bin')


class Sink:
    """The sink in a subprocess; its port and its stored messages."""

    def __init__(self, out_dir, *extra):
        self.out_dir = out_dir
        self.proc = subprocess.Popen([sys.executable, SINK, '0', out_dir, *extra],
                                     stdout=subprocess.PIPE, text=True)
        line = self.proc.stdout.readline()
        if not line.startswith('AZMAIL_SINK_READY'):
            self.stop()
            raise Failure(f'the sink did not start: {line!r}')
        self.port = int(line.split()[1])
        self.cert = None
        if '--tls-selfsigned' in extra:
            cert_line = self.proc.stdout.readline()
            self.cert = cert_line.split(None, 1)[1].strip()

    def messages(self):
        """[(raw bytes, envelope dict)] in arrival order."""
        out = []
        for name in sorted(os.listdir(self.out_dir)):
            if name.endswith('.eml'):
                stem = os.path.join(self.out_dir, name[:-4])
                with open(stem + '.eml', 'rb') as f:
                    raw = f.read()
                with open(stem + '.json', encoding='utf-8') as f:
                    out.append((raw, json.load(f)))
        return out

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()


class Runner:
    def __init__(self, binary, prefix):
        self.binary = binary
        self.prefix = shlex.split(prefix) if prefix else []
        env = dict(os.environ)
        libs = [os.path.join(REPO, 'target', 'release'), os.path.join(REPO, 'target', 'azul-lib')]
        for var in ('DYLD_LIBRARY_PATH', 'LD_LIBRARY_PATH'):
            env[var] = os.pathsep.join(libs + [env[var]] if env.get(var) else libs)
        self.env = env

    def send(self, data, *args, env=None):
        """Runs azmail-send (with `env` added to its environment); returns (exit code, the
        AZMAIL_SEND lines, all output)."""
        cmd = self.prefix + [self.binary, '--data', data, '--account', ACCOUNT, *args]
        proc = subprocess.run(cmd, env=dict(self.env, **(env or {})), capture_output=True,
                              text=True, timeout=180)
        output = proc.stdout + proc.stderr
        lines = [l for l in proc.stdout.splitlines() if l.startswith('AZMAIL_SEND')]
        return proc.returncode, lines, output


def account_dir(data):
    return os.path.join(data, ACCOUNT)


def sent_index(data):
    path = os.path.join(account_dir(data), 'mail', 'sent', 'index.jsonl')
    if not os.path.isfile(path):
        return []
    with open(path, encoding='utf-8') as f:
        return [json.loads(l) for l in f if l.strip()]


def outbox(data):
    path = os.path.join(account_dir(data), 'outbox')
    return sorted(os.listdir(path)) if os.path.isdir(path) else []


def parse(raw):
    return email.message_from_bytes(raw, policy=email.policy.default)


def base_args(port):
    return ['--smtp', f'127.0.0.1:{port}', '--tls', 'off',
            '--from', 'Ada Lovelace <ada@example.org>']


# ---- cases ----

def case_smtp(run, work):
    data, sink = os.path.join(work, 'data'), Sink(os.path.join(work, 'sink'))
    attachment = os.path.join(work, 'plan.txt')
    with open(attachment, 'w', encoding='utf-8') as f:
        f.write('step 1\nstep 2\n')
    try:
        code, lines, out = run.send(
            data, *base_args(sink.port),
            '--to', 'Ben Okafor <ben@example.net>', '--cc', 'cy@example.com',
            '--bcc', 'hidden@example.com', '--subject', 'Grüße aus Köln',
            '--text', 'Hallo Ben,\n.config is attached\nAda', '--html', '<p>Hallo Ben,</p>',
            '--attach', attachment + '=text/plain', '--in-reply-to', '<m0@example.net>',
            '--reference', '<m0@example.net>')
        check(code == 0 and lines and lines[0].startswith('AZMAIL_SEND sent '), out)
        message_id = lines[0].split()[2]
        got = sink.messages()
        check(len(got) == 1, f'the sink holds {len(got)} messages')
        raw, envelope = got[0]
        check(envelope['mail_from'] == 'ada@example.org', envelope)
        check(sorted(envelope['rcpt_to']) == ['ben@example.net', 'cy@example.com',
                                               'hidden@example.com'], envelope)
        check(not envelope['bare_line_ends'], 'a line without CRLF reached the sink')
        check(b'hidden@example.com' not in raw, 'the Bcc address is in the message')
        msg = parse(raw)
        check(msg['From'] == 'Ada Lovelace <ada@example.org>', msg['From'])
        check(msg['To'] == 'Ben Okafor <ben@example.net>', msg['To'])
        check(msg['Cc'] == 'cy@example.com', msg['Cc'])
        check(str(msg['Subject']) == 'Grüße aus Köln', msg['Subject'])
        check(b'Subject: =?utf-8?B?' in raw, 'the subject is not an RFC 2047 encoded word')
        check(msg['MIME-Version'] == '1.0', msg['MIME-Version'])
        check(msg['Message-ID'] == f'<{message_id}>', (msg['Message-ID'], message_id))
        check(msg['In-Reply-To'] == '<m0@example.net>', msg['In-Reply-To'])
        check(msg.get_content_type() == 'multipart/mixed', msg.get_content_type())
        text = msg.get_body(preferencelist=('plain',)).get_content()
        check('\n.config is attached\n' in text.replace('\r\n', '\n'), repr(text))
        html = msg.get_body(preferencelist=('html',)).get_content()
        check('Hallo Ben' in html, html)
        attachments = list(msg.iter_attachments())
        check(len(attachments) == 1 and attachments[0].get_filename() == 'plan.txt',
              [a.get_filename() for a in attachments])
        check(attachments[0].get_content().replace('\r\n', '\n') == 'step 1\nstep 2\n',
              repr(attachments[0].get_content()))
        index = sent_index(data)
        check(len(index) == 1 and index[0]['message_id'] == message_id, index)
        with open(os.path.join(account_dir(data), index[0]['path']), 'rb') as f:
            check(f.read() == raw, 'the Sent copy differs from what the server got')
        check(outbox(data) == [], outbox(data))
    finally:
        sink.stop()


def case_rejected(run, work):
    data = os.path.join(work, 'data')
    sink = Sink(os.path.join(work, 'sink'), '--reject', 'nobody@example.net=550 5.1.1 no such user')
    try:
        code, lines, out = run.send(data, *base_args(sink.port), '--to', 'ben@example.net',
                                    '--to', 'nobody@example.net', '--subject', 'x', '--text', 'x')
        check(code == 1 and lines and lines[0].startswith('AZMAIL_SEND failed '), out)
        check('nobody@example.net' in lines[0] and '5.1.1' in lines[0], lines[0])
        check(len(sink.messages()) == 1, 'the accepted recipient did not get it')
        check(len(sent_index(data)) == 1, 'a mail some recipients got belongs in Sent')
        policy_path = os.path.join(account_dir(data), 'send_policy.json')
        if os.path.isfile(policy_path):
            with open(policy_path, encoding='utf-8') as f:
                policy = json.load(f)
            check('example.net' not in policy['domains'], policy)
    finally:
        sink.stop()


def case_queued(run, work):
    data = os.path.join(work, 'data')
    busy = Sink(os.path.join(work, 'busy'), '--data-reply', '451 4.3.0 try again later')
    try:
        code, lines, out = run.send(data, *base_args(busy.port), '--to', 'ben@example.net',
                                    '--subject', 'later', '--text', 'x')
        check(code == 2 and lines and lines[0].startswith('AZMAIL_SEND queued '), out)
        check('451' in lines[0], lines[0])
        files = outbox(data)
        check(len(files) == 2 and files[0].endswith('.eml') and files[1].endswith('.json'), files)
        with open(os.path.join(account_dir(data), 'outbox', files[1]), encoding='utf-8') as f:
            entry = json.load(f)
        check(entry['state'] == 'queued' and entry['attempts'] == 1, entry)
        check(entry['recipients'][0]['state'] == 'pending', entry)
        check(sent_index(data) == [], 'a queued mail is not in Sent')
    finally:
        busy.stop()
    healthy = Sink(os.path.join(work, 'healthy'))
    try:
        code, lines, out = run.send(data, '--retry', '--force', '--smtp',
                                    f'127.0.0.1:{healthy.port}', '--tls', 'off')
        check(code == 0 and len(lines) == 1 and ' sent ' in lines[0], out)
        check(len(healthy.messages()) == 1, 'the retry did not reach the server')
        check(outbox(data) == [], outbox(data))
        check(len(sent_index(data)) == 1, sent_index(data))
    finally:
        healthy.stop()


def case_direct(run, work):
    data, sink = os.path.join(work, 'data'), Sink(os.path.join(work, 'sink'))
    try:
        code, lines, out = run.send(
            data, '--direct', '--direct-port', str(sink.port), '--tls', 'off',
            '--from', 'ada@example.org', '--to', 'ann@localhost', '--subject', 'direct',
            '--text', 'straight to the MX')
        check(code == 0 and lines and lines[0].startswith('AZMAIL_SEND sent '), out)
        got = sink.messages()
        check(len(got) == 1 and got[0][1]['rcpt_to'] == ['ann@localhost'], got)
        check(got[0][1]['helo'] == 'example.org', got[0][1])
    finally:
        sink.stop()


def case_policy(run, work):
    data = os.path.join(work, 'data')
    code, lines, out = run.send(data, '--direct', '--from', 'ada@example.org',
                                '--to', 'someone@gmail.com', '--subject', 'x', '--text', 'x')
    check(code == 2 and lines and lines[0].startswith('AZMAIL_SEND queued '), out)
    check('trusted relay' in lines[0], lines[0])
    with open(os.path.join(account_dir(data), 'send_policy.json'), encoding='utf-8') as f:
        policy = json.load(f)
    check(policy['domains']['gmail.com']['route'] == 'relay', policy)
    check(policy['domains']['outlook.com']['route'] == 'direct', policy)


def case_starttls(run, work):
    data = os.path.join(work, 'data')
    sink = Sink(os.path.join(work, 'sink'), '--tls-selfsigned')
    try:
        code, lines, out = run.send(
            data, '--smtp', f'127.0.0.1:{sink.port}', '--tls', 'required', '--ca', sink.cert,
            '--from', 'ada@example.org', '--to', 'ben@example.net', '--subject', 'tls',
            '--text', 'over TLS')
        check(code == 0 and lines and lines[0].startswith('AZMAIL_SEND sent '), out)
        got = sink.messages()
        check(len(got) == 1 and got[0][1]['tls'] is True, got and got[0][1])
    finally:
        sink.stop()


def relaxed_body_hash(body):
    """RFC 6376 3.4.4 relaxed body canonicalization, SHA-256, base64."""
    lines = body.split(b'\r\n')
    out = []
    for line in lines:
        line = re.sub(rb'[ \t]+', b' ', line).rstrip(b' \t')
        out.append(line)
    while out and out[-1] == b'':
        out.pop()
    canonical = b''.join(l + b'\r\n' for l in out)
    return base64.b64encode(hashlib.sha256(canonical).digest()).decode()


def relaxed_header(field):
    """RFC 6376 3.4.2 relaxed header canonicalization of one raw field (folds included, no
    final CRLF)."""
    name, _, value = field.partition(b':')
    value = re.sub(rb'\r\n(?=[ \t])', b'', value)
    value = re.sub(rb'[ \t]+', b' ', value).strip(b' ')
    return name.strip().lower() + b':' + value


def dkim_tags(text):
    tags = {}
    for part in text.split(';'):
        if '=' in part:
            name, _, value = part.partition('=')
            tags[name.strip()] = value.strip()
    return tags


def dkim_verify(raw, public_key_b64, work, domain='example.org'):
    """An independent DKIM verifier (RFC 6376, written here, not micromail's): the signature
    header's tags, the relaxed body hash, the relaxed canonicalization of the signed header
    fields (picked bottom up) and of the DKIM-Signature with b= emptied, and the RSA-SHA256
    signature checked by OpenSSL against the published key (`p=`, SubjectPublicKeyInfo).
    Returns None when the mail verifies, else why. With dkimpy installed it must agree."""
    header_end = raw.index(b'\r\n\r\n')
    head, body = raw[:header_end], raw[header_end + 4:]
    fields = re.split(rb'\r\n(?![ \t])', head)
    names = [f.partition(b':')[0].strip().lower() for f in fields]
    if b'dkim-signature' not in names:
        return 'no DKIM-Signature'
    sig_field = fields[names.index(b'dkim-signature')]
    tags = dkim_tags(re.sub(r'\s+', ' ', sig_field.partition(b':')[2].decode()))
    if tags.get('v') != '1' or tags.get('a') != 'rsa-sha256':
        return f'unexpected v= / a=: {tags}'
    if tags.get('d') != domain:
        return f'd={tags.get("d")}, not {domain}'
    if tags.get('c', 'simple/simple') != 'relaxed/relaxed':
        return f'this checker reads relaxed/relaxed only, not c={tags.get("c")}'
    bh = tags.get('bh', '').replace(' ', '')
    if bh != relaxed_body_hash(body):
        return f'bh={bh}, the body hashes to {relaxed_body_hash(body)}'
    # The signed fields, each instance taken from the bottom up (RFC 6376 5.4.2).
    used = set()
    data = b''
    for name in [n.strip().lower().encode() for n in tags.get('h', '').split(':') if n.strip()]:
        for i in range(len(fields) - 1, -1, -1):
            if names[i] == name and i not in used and fields[i] is not sig_field:
                used.add(i)
                data += relaxed_header(fields[i]) + b'\r\n'
                break
    unsigned = re.sub(rb'((?:^|;)\s*b\s*=)[^;]*', rb'\1', sig_field)
    data += relaxed_header(unsigned)
    signature = base64.b64decode(re.sub(r'\s+', '', tags.get('b', '')))
    der = base64.b64decode(public_key_b64)
    pem = '-----BEGIN PUBLIC KEY-----\n' + '\n'.join(
        base64.b64encode(der).decode()[i:i + 64] for i in range(0, len(base64.b64encode(der)), 64)
    ) + '\n-----END PUBLIC KEY-----\n'
    paths = {k: os.path.join(work, f'verify.{k}') for k in ('pem', 'sig', 'data')}
    with open(paths['pem'], 'w', encoding='ascii') as f:
        f.write(pem)
    with open(paths['sig'], 'wb') as f:
        f.write(signature)
    with open(paths['data'], 'wb') as f:
        f.write(data)
    proc = subprocess.run(['openssl', 'dgst', '-sha256', '-verify', paths['pem'], '-signature',
                           paths['sig'], paths['data']], capture_output=True, text=True)
    if proc.returncode != 0 or 'Verified OK' not in proc.stdout:
        return f'OpenSSL does not verify the signature: {proc.stdout.strip()} {proc.stderr.strip()}'
    try:
        import dkim  # dkimpy, when installed: a second independent opinion
    except ImportError:
        return None
    record = f'v=DKIM1; k=rsa; p={public_key_b64}'.encode()
    if not dkim.verify(raw, dnsfunc=lambda name, timeout=5: record):
        return 'OpenSSL verifies, dkimpy does not'
    return None


def spki_of(private_pem):
    """The SubjectPublicKeyInfo (base64) of a private key file, by OpenSSL."""
    der = subprocess.run(['openssl', 'pkey', '-in', private_pem, '-pubout', '-outform', 'DER'],
                         check=True, capture_output=True).stdout
    return base64.b64encode(der).decode()


def case_dkim(run, work):
    data, sink = os.path.join(work, 'data'), Sink(os.path.join(work, 'sink'))
    key = os.path.join(work, 'dkim.pem')
    subprocess.run(['openssl', 'genrsa', '-out', key, '2048'], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        code, lines, out = run.send(
            data, *base_args(sink.port), '--to', 'ben@example.net', '--subject', 'signed',
            '--text', 'Signed  text \nwith   spaces\n\n\n', '--dkim-domain', 'example.org',
            '--dkim-selector', 'azmail', '--dkim-key', key)
        check(code == 0 and lines and lines[0].startswith('AZMAIL_SEND sent '), out)
        raw = sink.messages()[0][0]
        check(raw.startswith(b'DKIM-Signature: v=1; a=rsa-sha256; s=azmail; d=example.org;'),
              raw[:200])
        header_end = raw.index(b'\r\n\r\n')
        unfolded = re.sub(rb'\r\n[ \t]+', b' ', raw[:header_end])
        signature = unfolded.split(b'\r\n')[0].decode()
        bh = re.search(r'bh=([^;]+);', signature).group(1).replace(' ', '')
        expected = relaxed_body_hash(raw[header_end + 4:])
        check(bh == expected, f'bh={bh}, the body hashes to {expected}')
        problem = dkim_verify(raw, spki_of(key), work)
        check(problem is None, f'the signature does not verify: {problem}')
        check(b'PRIVATE KEY' not in raw, 'the key is in the message')
        for root, _, files in os.walk(data):
            for name in files:
                with open(os.path.join(root, name), 'rb') as f:
                    check(b'PRIVATE KEY' not in f.read(), f'the key is in {name}')
    finally:
        sink.stop()


def case_dkim_generated(run, work):
    """The client-side key: made by AzMail (as the Sending page makes it), its DNS record
    printed; the mail signed with it verifies against exactly that record, and the record's
    key is the key file's public half (by OpenSSL)."""
    data, sink = os.path.join(work, 'data'), Sink(os.path.join(work, 'sink'))
    key = os.path.join(work, 'generated.pem')
    try:
        code, lines, out = run.send(
            data, *base_args(sink.port), '--to', 'ben@example.net', '--subject', 'own key',
            '--text', 'signed with a key made on this computer', '--dkim-generate', key)
        check(code == 0 and lines and lines[0].startswith('AZMAIL_SEND sent '), out)
        name = re.search(r'^AZMAIL_DKIM_NAME (\S+)$', out, re.M)
        value = re.search(r'^AZMAIL_DKIM_VALUE (.+)$', out, re.M)
        check(name and value, out)
        check(re.fullmatch(r'azmail\d{6}\._domainkey\.example\.org', name.group(1)),
              name.group(1))
        tags = dkim_tags(value.group(1))
        check(tags.get('v') == 'DKIM1' and tags.get('k') == 'rsa', value.group(1))
        published = tags.get('p', '')
        check(published.startswith('MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA'),
              'p= is not a 2048-bit SubjectPublicKeyInfo: ' + published[:60])
        check(published == spki_of(key), 'the record does not publish the key file\'s key')
        if os.name == 'posix':
            check(os.stat(key).st_mode & 0o077 == 0, 'the key file is readable by others')
        raw = sink.messages()[0][0]
        selector = name.group(1).split('.')[0]
        check(f's={selector};'.encode() in raw[:300], raw[:300])
        problem = dkim_verify(raw, published, work)
        check(problem is None, f'the signature does not verify against the record: {problem}')
    finally:
        sink.stop()


def free_port():
    """A port nothing listens on (bound, then closed)."""
    import socket
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def case_port25(run, work):
    """No exchanger answers and the probe cannot connect either: the connection is recorded as
    blocking port 25, the mail waits; within the hour a retry knocks nowhere; --ignore-policy
    (and a server that answers) sends it."""
    data, closed = os.path.join(work, 'data'), free_port()
    probe = f'127.0.0.1:{closed}'
    code, lines, out = run.send(
        data, '--direct', '--direct-port', str(closed), '--tls', 'off', '--port25-probe', probe,
        '--from', 'ada@example.org', '--to', 'ann@localhost', '--subject', 'blocked',
        '--text', 'x')
    check(code == 2 and lines and lines[0].startswith('AZMAIL_SEND queued '), out)
    check('port 25' in lines[0], lines[0])
    with open(os.path.join(account_dir(data), 'send_policy.json'), encoding='utf-8') as f:
        policy = json.load(f)
    check(policy.get('port25', {}).get('open') is False, policy)
    sink = Sink(os.path.join(work, 'sink'))
    try:
        code, lines, out = run.send(
            data, '--retry', '--force', '--direct', '--direct-port', str(sink.port), '--tls',
            'off', '--port25-probe', probe)
        check(code == 2 and lines and ' queued ' in lines[0], out)
        check(sink.messages() == [], 'a connection found blocked is not tried within the hour')
        code, lines, out = run.send(
            data, '--retry', '--force', '--direct', '--direct-port', str(sink.port), '--tls',
            'off', '--ignore-policy')
        check(code == 0 and lines and ' sent ' in lines[0], out)
        check(len(sink.messages()) == 1, 'the mail did not arrive')
    finally:
        sink.stop()


# ---- submission: the optional signed-in route ----

SECRET_VAR = 'AZMAIL_E2E_SUBMIT_SECRET'
SECRET = 'app-password-e2e-7f3a'


def write_account(data, port, auth='password'):
    """The account file submission reads its outgoing server, user name and kind of secret
    from (account.rs's format, version 1)."""
    os.makedirs(account_dir(data), exist_ok=True)
    account = {
        'format': 'azmail.account', 'version': 1, 'email': ACCOUNT, 'username': 'ada',
        'imap': {'host': '127.0.0.1', 'port': 993}, 'smtp': {'host': '127.0.0.1', 'port': port},
        'security': 'tls', 'auth': auth,
    }
    with open(os.path.join(account_dir(data), 'account.json'), 'w', encoding='utf-8') as f:
        json.dump(account, f, indent=2)


def no_secret_anywhere(data, out, secret):
    check(secret not in out, 'the secret is in the output')
    for folder, _, files in os.walk(data):
        for name in files:
            with open(os.path.join(folder, name), 'rb') as f:
                check(secret.encode() not in f.read(), f'the secret is in {name}')


def submit_once(run, work, sink_args, send_args, auth='password', secret=SECRET):
    """One mail through the submission route to a sink started with `sink_args`; returns
    (data folder, sink, exit code, lines, output); the caller stops the sink."""
    data = os.path.join(work, 'data')
    sink = Sink(os.path.join(work, 'sink'), '--tls-selfsigned', *sink_args)
    write_account(data, sink.port, auth)
    code, lines, out = run.send(
        data, '--submission', '--ca', sink.cert, '--password-env', SECRET_VAR, *send_args,
        '--from', 'Ada Lovelace <ada@example.org>', '--to', 'ben@example.net',
        '--bcc', 'dee@example.com', '--subject', 'signed in', '--text', 'through my provider',
        env={SECRET_VAR: secret})
    return data, sink, code, lines, out


def check_submitted(data, sink, code, lines, out, mechanism):
    check(code == 0 and lines and lines[0].startswith('AZMAIL_SEND sent '), out)
    got = sink.messages()
    check(len(got) == 1, got)
    raw, envelope = got[0]
    check(envelope['tls'] is True, envelope)
    check(envelope['auth_user'] == 'ada' and envelope['auth_mechanism'] == mechanism, envelope)
    check(envelope['rcpt_to'] == ['ben@example.net', 'dee@example.com'], envelope)
    check(b'dee@example.com' not in raw, 'Bcc only in the envelope')
    index = sent_index(data)
    check(len(index) == 1, index)
    with open(os.path.join(account_dir(data), index[0]['path']), 'rb') as f:
        check(f.read() == raw, 'Sent keeps the bytes the server got')
    check(outbox(data) == [], outbox(data))
    no_secret_anywhere(data, out, SECRET)


def case_submission(run, work):
    data, sink, code, lines, out = submit_once(
        run, work, ['--auth', f'ada={SECRET}', '--auth-mechs', 'PLAIN LOGIN'], [])
    try:
        check_submitted(data, sink, code, lines, out, 'PLAIN')
    finally:
        sink.stop()


def case_submission_implicit(run, work):
    data, sink, code, lines, out = submit_once(
        run, work, ['--implicit-tls', '--auth', f'ada={SECRET}', '--auth-mechs', 'LOGIN'],
        ['--tls', 'implicit'])
    try:
        check_submitted(data, sink, code, lines, out, 'LOGIN')
    finally:
        sink.stop()


def case_submission_xoauth2(run, work):
    data, sink, code, lines, out = submit_once(
        run, work, ['--auth', f'ada={SECRET}', '--auth-mechs', 'PLAIN LOGIN XOAUTH2'], [],
        auth='xoauth2')
    try:
        check_submitted(data, sink, code, lines, out, 'XOAUTH2')
    finally:
        sink.stop()


def case_submission_refused(run, work):
    wrong = 'wrong-password-e2e-91c2'
    data, sink, code, lines, out = submit_once(
        run, work, ['--auth', f'ada={SECRET}', '--auth-mechs', 'PLAIN LOGIN'], [], secret=wrong)
    try:
        check(code == 2 and lines and lines[0].startswith('AZMAIL_SEND queued '), out)
        check('535' in lines[0], lines[0])
        check(sink.messages() == [], 'nothing was handed over')
        no_secret_anywhere(data, out, wrong)
        files = outbox(data)
        check(len(files) == 2 and files[1].endswith('.json'), files)
        with open(os.path.join(account_dir(data), 'outbox', files[1]), encoding='utf-8') as f:
            entry = json.load(f)
        check(entry['state'] == 'queued' and entry['attempts'] == 0, entry)
        # The password is new: the next Send / Receive sends it.
        code, lines, out = run.send(data, '--retry', '--submission', '--ca', sink.cert,
                                    '--password-env', SECRET_VAR, env={SECRET_VAR: SECRET})
        check(code == 0 and len(lines) == 1 and ' sent ' in lines[0], out)
        check(len(sink.messages()) == 1, 'the retry did not reach the server')
        check(outbox(data) == [], outbox(data))
    finally:
        sink.stop()


CASES = {
    'smtp': (case_smtp, None),
    'rejected': (case_rejected, None),
    'queued': (case_queued, None),
    'direct': (case_direct, None),
    'policy': (case_policy, None),
    'starttls': (case_starttls, 'openssl'),
    'dkim': (case_dkim, 'openssl'),
    'dkim-generated': (case_dkim_generated, 'openssl'),
    'port25': (case_port25, None),
    'submission': (case_submission, 'openssl'),
    'submission-implicit': (case_submission_implicit, 'openssl'),
    'submission-xoauth2': (case_submission_xoauth2, 'openssl'),
    'submission-refused': (case_submission_refused, 'openssl'),
}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--bin')
    ap.add_argument('--case', action='append', choices=sorted(CASES))
    ap.add_argument('--keep', action='store_true', help='keep the temporary folders')
    ap.add_argument('--runner', default='', help='a command prefix, e.g. the capped runner')
    args = ap.parse_args()
    run = Runner(find_bin(args.bin), args.runner)
    root = tempfile.mkdtemp(prefix='azmail-send-test-')
    failed = []
    for name in args.case or list(CASES):
        case, needs = CASES[name]
        if needs and not shutil.which(needs):
            print(f'SKIP {name} (needs {needs})')
            continue
        work = os.path.join(root, name)
        os.makedirs(work)
        try:
            case(run, work)
            print(f'PASS {name}')
        except (Failure, subprocess.TimeoutExpired, OSError, KeyError, ValueError) as e:
            print(f'FAIL {name}: {e}')
            failed.append(name)
    if failed or args.keep:
        print(f'files: {root}')
    else:
        shutil.rmtree(root, ignore_errors=True)
    print('azmail_send_test: ' + ('FAILED ' + ', '.join(failed) if failed else 'all passed'))
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
