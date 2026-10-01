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
  dkim          (needs openssl) --dkim-domain/--dkim-key: a DKIM-Signature with d= and s=, and
                bh= the SHA-256 of the relaxed body

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

    def send(self, data, *args):
        """Runs azmail-send; returns (exit code, the AZMAIL_SEND lines, all output)."""
        cmd = self.prefix + [self.binary, '--data', data, '--account', ACCOUNT, *args]
        proc = subprocess.run(cmd, env=self.env, capture_output=True, text=True, timeout=180)
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
        check(b'PRIVATE KEY' not in raw, 'the key is in the message')
        for root, _, files in os.walk(data):
            for name in files:
                with open(os.path.join(root, name), 'rb') as f:
                    check(b'PRIVATE KEY' not in f.read(), f'the key is in {name}')
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
