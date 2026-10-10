#!/usr/bin/env python3
"""A small IMAP4rev1 server for testing AzMail, standard library only.

It serves a folder of mailboxes: every directory under ROOT is a mailbox named by its path
(`INBOX`, `Spam`, `Work/Projects`, `Entwürfe` - listed in modified UTF-7), and every `*.eml` file
in it is a message. UIDs are given in file-name order when a mailbox is first opened, and a file
added later gets the next UID, as on a real server. UIDVALIDITY is 1, or the number in the
mailbox's `.uidvalidity` file; flags come from its `.flags.json` (`{"0001.eml": ["\\\\Seen"]}`).
Messages are served with CRLF line ends. INTERNALDATE is the message's Date header (else the
file's time).

The IMAP subset: CAPABILITY, NOOP, LOGOUT, LOGIN, AUTHENTICATE PLAIN and XOAUTH2 (the password is
the token), LIST, STATUS, SELECT, EXAMINE, CLOSE, UNSELECT, CHECK, SEARCH / UID SEARCH (`ALL`,
`UID <set>`), FETCH / UID FETCH (UID, FLAGS, RFC822.SIZE, INTERNALDATE, BODY[], BODY.PEEK[],
RFC822, RFC822.HEADER, BODY.PEEK[HEADER]). Mailbox names with special use (Spam or Junk, Sent,
Drafts, Trash, Archive) get their RFC 6154 attribute unless --no-special-use.

Plain TCP by default; --tls serves implicit TLS (IMAPS) with a self-signed certificate made at
start with the `openssl` tool, for 127.0.0.1 and localhost.

    python3 imap_server.py --root DIR [--port 0] [--user U] [--password P] [--log FILE] [--tls]

On stdout: `IMAP_SERVER_PORT <port>`, and with --tls `IMAP_SERVER_CA <certificate file>`. The log
(--log) has one JSON object per command that matters to a test (sign-ins, selects, searches,
fetches: `{"command": "UID FETCH", "mailbox": "INBOX", "uids": [1, 2], "body": true}`); it never
holds a password or a token.
"""
import argparse
import base64
import email.utils
import json
import os
import re
import shutil
import socketserver
import ssl
import subprocess
import sys
import tempfile
import threading
import time

CAPABILITIES = 'IMAP4rev1 AUTH=PLAIN AUTH=XOAUTH2 SPECIAL-USE'
SPECIAL_USE = {
    'spam': '\\Junk',
    'junk': '\\Junk',
    'sent': '\\Sent',
    'drafts': '\\Drafts',
    'trash': '\\Trash',
    'archive': '\\Archive',
}
MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
SYSTEM_FLAGS = '(\\Answered \\Flagged \\Deleted \\Seen \\Draft)'


# ---- modified UTF-7 (RFC 3501, 5.1.3) ----

def encode_mutf7(name):
    """A mailbox name in modified UTF-7."""
    out = []
    shifted = []

    def flush():
        if shifted:
            raw = ''.join(shifted).encode('utf-16-be')
            b64 = base64.b64encode(raw).decode('ascii').rstrip('=').replace('/', ',')
            out.append('&' + b64 + '-')
            shifted.clear()

    for ch in name:
        if 0x20 <= ord(ch) <= 0x7e:
            flush()
            out.append('&-' if ch == '&' else ch)
        else:
            shifted.append(ch)
    flush()
    return ''.join(out)


def decode_mutf7(name):
    """The UTF-8 name of a modified UTF-7 one (a malformed name comes back as it is)."""
    def run(match):
        body = match.group(1)
        if body == '':
            return '&'
        b64 = body.replace(',', '/')
        b64 += '=' * (-len(b64) % 4)
        try:
            return base64.b64decode(b64).decode('utf-16-be')
        except Exception:
            return match.group(0)
    return re.sub(r'&([A-Za-z0-9+,]*)-', run, name)


# ---- messages ----

def load_message(path):
    """A message's bytes as served: CRLF line ends."""
    with open(path, 'rb') as f:
        data = f.read()
    return re.sub(rb'(?<!\r)\n', b'\r\n', data)


def internal_date(data, path):
    """INTERNALDATE: the Date header, else the file's time, as `30-Sep-2026 10:42:00 +0200`."""
    when = None
    match = re.search(rb'(?im)^Date:[ \t]*(.+?)\r?$', data.split(b'\r\n\r\n', 1)[0])
    if match:
        try:
            when = email.utils.parsedate_to_datetime(match.group(1).decode('latin-1').strip())
        except (TypeError, ValueError):
            when = None
    if when is None or when.tzinfo is None:
        import datetime
        when = datetime.datetime.fromtimestamp(os.path.getmtime(path), datetime.timezone.utc)
    offset = when.utcoffset()
    minutes = int(offset.total_seconds() // 60) if offset else 0
    sign = '+' if minutes >= 0 else '-'
    minutes = abs(minutes)
    return (f'{when.day:02d}-{MONTHS[when.month - 1]}-{when.year:04d} '
            f'{when.hour:02d}:{when.minute:02d}:{when.second:02d} '
            f'{sign}{minutes // 60:02d}{minutes % 60:02d}')


class Mailbox:
    """One directory of `.eml` files."""

    def __init__(self, name, path):
        self.name = name  # UTF-8, '/' between levels
        self.path = path
        self.uids = {}  # file name -> UID
        self.next_uid = 1
        self.lock = threading.Lock()

    def uidvalidity(self):
        try:
            with open(os.path.join(self.path, '.uidvalidity')) as f:
                return int(f.read().strip())
        except (OSError, ValueError):
            return 1

    def flags(self):
        try:
            with open(os.path.join(self.path, '.flags.json'), encoding='utf-8') as f:
                return json.load(f)
        except (OSError, ValueError):
            return {}

    def scan(self):
        """The messages, `[(uid, path)]` by UID; new files get the next UIDs."""
        with self.lock:
            try:
                names = sorted(n for n in os.listdir(self.path)
                               if n.endswith('.eml') and not n.startswith('.'))
            except OSError:
                names = []
            for n in names:
                if n not in self.uids:
                    self.uids[n] = self.next_uid
                    self.next_uid += 1
            present = set(names)
            self.uids = {n: u for n, u in self.uids.items() if n in present}
            return sorted((u, os.path.join(self.path, n)) for n, u in self.uids.items())


class Store:
    """Every mailbox under a root folder."""

    def __init__(self, root, special_use=True):
        self.root = root
        self.special_use = special_use
        self.mailboxes = {}  # UTF-8 name -> Mailbox
        self.lock = threading.Lock()

    def refresh(self):
        with self.lock:
            for dirpath, dirnames, _ in os.walk(self.root):
                dirnames[:] = sorted(d for d in dirnames if not d.startswith('.'))
                for d in dirnames:
                    path = os.path.join(dirpath, d)
                    name = os.path.relpath(path, self.root).replace(os.sep, '/')
                    if name not in self.mailboxes:
                        self.mailboxes[name] = Mailbox(name, path)
            return dict(self.mailboxes)

    def attributes(self, name, all_names):
        children = any(other.startswith(name + '/') for other in all_names)
        attrs = ['\\HasChildren' if children else '\\HasNoChildren']
        if self.special_use:
            use = SPECIAL_USE.get(name.rsplit('/', 1)[-1].lower())
            if use:
                attrs.append(use)
        return attrs

    def find(self, wire_name):
        name = decode_mutf7(wire_name)
        mailboxes = self.refresh()
        if name.upper() == 'INBOX':
            for key in mailboxes:
                if key.upper() == 'INBOX':
                    return mailboxes[key]
        return mailboxes.get(name)


# ---- the protocol ----

def quote(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'


def tokenize(line):
    """IMAP arguments: atoms, quoted strings, parenthesized lists (kept whole), and bracketed
    sections inside atoms (`BODY.PEEK[HEADER.FIELDS (A B)]`)."""
    tokens = []
    i = 0
    while i < len(line):
        c = line[i]
        if c == ' ':
            i += 1
        elif c == '"':
            j = i + 1
            buf = []
            while j < len(line) and line[j] != '"':
                if line[j] == '\\' and j + 1 < len(line):
                    buf.append(line[j + 1])
                    j += 2
                else:
                    buf.append(line[j])
                    j += 1
            tokens.append(''.join(buf))
            i = j + 1
        elif c == '(':
            depth = 0
            j = i
            while j < len(line):
                if line[j] == '(':
                    depth += 1
                elif line[j] == ')':
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            tokens.append(line[i:j + 1])
            i = j + 1
        else:
            j = i
            while j < len(line) and line[j] != ' ':
                if line[j] == '[':
                    close = line.find(']', j)
                    j = len(line) if close < 0 else close
                j += 1
            tokens.append(line[i:j])
            i = j
    return tokens


def parse_set(spec, values):
    """The members of `values` (ascending ints) a sequence set names; `*` is the largest."""
    if not values:
        return []
    largest = values[-1]
    chosen = set()
    for part in spec.split(','):
        if ':' in part:
            a, b = part.split(':', 1)
        else:
            a = b = part
        try:
            lo = largest if a == '*' else int(a)
            hi = largest if b == '*' else int(b)
        except ValueError:
            raise ValueError(f'bad sequence set {spec!r}')
        lo, hi = min(lo, hi), max(lo, hi)
        chosen.update(v for v in values if lo <= v <= hi)
    return sorted(chosen)


class Session(socketserver.StreamRequestHandler):
    """One client connection."""

    def setup(self):
        if isinstance(self.request, ssl.SSLSocket):
            self.request.do_handshake()
        super().setup()

    def send(self, text):
        self.wfile.write(text.encode('utf-8') + b'\r\n')

    def log(self, **entry):
        self.server.log(entry)

    def readline(self):
        line = self.rfile.readline(1 << 20)
        if not line:
            return None
        return line.rstrip(b'\r\n').decode('utf-8', 'surrogateescape')

    def read_command(self):
        """One command line, with synchronizing literals read in and quoted."""
        line = self.readline()
        if line is None:
            return None
        while True:
            match = re.search(r'\{(\d+)(\+?)\}$', line)
            if not match:
                return line
            size = int(match.group(1))
            if not match.group(2):
                self.send('+ Ready for literal')
                self.wfile.flush()
            data = self.rfile.read(size).decode('utf-8', 'surrogateescape')
            rest = self.readline() or ''
            line = line[:match.start()] + quote(data) + rest

    def handle(self):
        self.user = None
        self.selected = None
        self.read_only = False
        self.send(f'* OK [CAPABILITY {CAPABILITIES}] AzMail test IMAP server ready')
        while True:
            try:
                line = self.read_command()
            except (OSError, ValueError):
                return
            if line is None:
                return
            tokens = tokenize(line)
            if len(tokens) < 2:
                self.send('* BAD Missing command')
                continue
            tag, command, args = tokens[0], tokens[1].upper(), tokens[2:]
            if command == 'UID' and args:
                command, args = 'UID ' + args[0].upper(), args[1:]
            try:
                if not self.dispatch(tag, command, args):
                    return
            except ValueError as e:
                self.send(f'{tag} BAD {e}')
            except OSError:
                return

    def dispatch(self, tag, command, args):
        """Runs one command; False ends the connection."""
        if command == 'CAPABILITY':
            self.send(f'* CAPABILITY {CAPABILITIES}')
        elif command in ('NOOP', 'CHECK'):
            pass
        elif command == 'LOGOUT':
            self.send('* BYE Logging out')
            self.send(f'{tag} OK LOGOUT completed')
            return False
        elif command == 'LOGIN':
            if len(args) != 2:
                raise ValueError('LOGIN takes a user and a password')
            return self.signed_in(tag, 'LOGIN', args[0], args[1])
        elif command == 'AUTHENTICATE':
            return self.authenticate(tag, args)
        elif self.user is None:
            self.send(f'{tag} BAD {command} is not allowed before signing in')
            return True
        elif command == 'LIST':
            self.list(args)
        elif command == 'STATUS':
            if not self.status(tag, args):
                return True
        elif command in ('SELECT', 'EXAMINE'):
            if not self.select(tag, command, args):
                return True
            state = 'READ-ONLY' if self.read_only else 'READ-WRITE'
            self.send(f'{tag} OK [{state}] {command} completed')
            return True
        elif command in ('CLOSE', 'UNSELECT'):
            self.selected = None
        elif self.selected is None:
            self.send(f'{tag} BAD No mailbox selected')
            return True
        elif command in ('SEARCH', 'UID SEARCH'):
            self.search(command, args)
        elif command in ('FETCH', 'UID FETCH'):
            self.fetch(command, args)
        else:
            self.send(f'{tag} BAD Unknown command {command}')
            return True
        self.send(f'{tag} OK {command} completed')
        return True

    # -- signing in --

    def signed_in(self, tag, how, user, secret):
        ok = user == self.server.user and secret == self.server.password
        self.log(command=how, user=user, ok=ok)
        if ok:
            self.user = user
            self.send(f'{tag} OK [CAPABILITY {CAPABILITIES}] Signed in')
        else:
            self.send(f'{tag} NO [AUTHENTICATIONFAILED] Invalid credentials')
        return True

    def authenticate(self, tag, args):
        if not args:
            raise ValueError('AUTHENTICATE needs a mechanism')
        mechanism = args[0].upper()
        if mechanism not in ('PLAIN', 'XOAUTH2'):
            self.send(f'{tag} NO Unsupported mechanism')
            return True
        if len(args) > 1:
            response = args[1]
        else:
            self.send('+ ')
            self.wfile.flush()
            response = self.readline()
            if response is None:
                return False
        if response.strip() == '*':
            self.send(f'{tag} BAD AUTHENTICATE cancelled')
            return True
        try:
            decoded = base64.b64decode(response.strip(), validate=True).decode('utf-8')
        except (ValueError, UnicodeDecodeError):
            self.send(f'{tag} BAD Invalid base64')
            return True
        if mechanism == 'PLAIN':
            parts = decoded.split('\0')
            if len(parts) != 3:
                self.send(f'{tag} BAD Invalid PLAIN response')
                return True
            return self.signed_in(tag, 'AUTHENTICATE PLAIN', parts[1], parts[2])
        fields = dict(
            f.split('=', 1) for f in decoded.split('\x01') if '=' in f)
        user = fields.get('user', '')
        token = fields.get('auth', '')
        token = token[len('Bearer '):] if token.startswith('Bearer ') else None
        if user == self.server.user and token == self.server.password:
            return self.signed_in(tag, 'AUTHENTICATE XOAUTH2', user, token)
        # XOAUTH2 answers a failure with an error challenge; the client ends it with an empty line.
        error = json.dumps({'status': '401', 'schemes': 'Bearer', 'scope': 'mail'})
        self.send('+ ' + base64.b64encode(error.encode()).decode())
        self.wfile.flush()
        self.readline()
        self.log(command='AUTHENTICATE XOAUTH2', user=user, ok=False)
        self.send(f'{tag} NO [AUTHENTICATIONFAILED] Invalid credentials')
        return True

    # -- mailboxes --

    def list(self, args):
        if len(args) != 2:
            raise ValueError('LIST takes a reference and a pattern')
        reference, pattern = decode_mutf7(args[0]), decode_mutf7(args[1])
        regex = '^' + ''.join(
            '.*' if c == '*' else '[^/]*' if c == '%' else re.escape(c)
            for c in reference + pattern) + '$'
        mailboxes = self.server.store.refresh()
        names = sorted(mailboxes)
        for name in names:
            if re.match(regex, name, re.IGNORECASE if name.upper() == 'INBOX' else 0):
                attrs = ' '.join(self.server.store.attributes(name, names))
                self.send(f'* LIST ({attrs}) "/" {quote(encode_mutf7(name))}')

    def status(self, tag, args):
        if not args:
            raise ValueError('STATUS takes a mailbox')
        mailbox = self.server.store.find(args[0])
        if mailbox is None:
            self.send(f'{tag} NO No such mailbox')
            return False
        messages = mailbox.scan()
        self.send(f'* STATUS {quote(encode_mutf7(mailbox.name))} (MESSAGES {len(messages)} '
                  f'UIDNEXT {mailbox.next_uid} UIDVALIDITY {mailbox.uidvalidity()} UNSEEN 0)')
        return True

    def select(self, tag, command, args):
        if not args:
            raise ValueError(f'{command} takes a mailbox')
        mailbox = self.server.store.find(args[0])
        self.selected = None
        if mailbox is None:
            self.send(f'{tag} NO No such mailbox')
            return False
        messages = mailbox.scan()
        self.selected = mailbox
        self.read_only = command == 'EXAMINE'
        self.log(command=command, mailbox=mailbox.name)
        self.send(f'* FLAGS {SYSTEM_FLAGS}')
        self.send(f'* {len(messages)} EXISTS')
        self.send('* 0 RECENT')
        self.send(f'* OK [UIDVALIDITY {mailbox.uidvalidity()}] UIDs valid')
        self.send(f'* OK [UIDNEXT {mailbox.next_uid}] Predicted next UID')
        self.send(f'* OK [PERMANENTFLAGS {SYSTEM_FLAGS}] Limited')
        return True

    # -- messages --

    def numbered(self):
        """`[(sequence number, uid, path)]` of the selected mailbox."""
        return [(i + 1, uid, path) for i, (uid, path) in enumerate(self.selected.scan())]

    def search(self, command, args):
        messages = self.numbered()
        by_uid = command == 'UID SEARCH'
        chosen = [m for m in messages]
        i = 0
        while i < len(args):
            key = args[i].upper()
            if key == 'ALL':
                i += 1
            elif key == 'UID' and i + 1 < len(args):
                uids = parse_set(args[i + 1], [m[1] for m in messages])
                chosen = [m for m in chosen if m[1] in uids]
                i += 2
            elif re.match(r'^[\d*:,]+$', key):
                seqs = parse_set(args[i], [m[0] for m in messages])
                chosen = [m for m in chosen if m[0] in seqs]
                i += 1
            else:
                raise ValueError(f'unsupported search key {args[i]}')
        found = [m[1] if by_uid else m[0] for m in chosen]
        self.log(command=command, mailbox=self.selected.name, found=found)
        self.send('* SEARCH' + ''.join(f' {n}' for n in found))

    def fetch(self, command, args):
        if len(args) < 2:
            raise ValueError(f'{command} takes a set and items')
        by_uid = command == 'UID FETCH'
        messages = self.numbered()
        spec = args[0]
        items_text = ' '.join(args[1:])
        if items_text.startswith('(') and items_text.endswith(')'):
            items_text = items_text[1:-1]
        items = [item.upper() for item in tokenize(items_text)]
        macros = {'ALL': ['FLAGS', 'INTERNALDATE', 'RFC822.SIZE'],
                  'FAST': ['FLAGS', 'INTERNALDATE', 'RFC822.SIZE'],
                  'FULL': ['FLAGS', 'INTERNALDATE', 'RFC822.SIZE']}
        if len(items) == 1 and items[0] in macros:
            items = macros[items[0]]
        if by_uid and 'UID' not in items:
            items.insert(0, 'UID')
        if by_uid:
            wanted = set(parse_set(spec, [m[1] for m in messages]))
            chosen = [m for m in messages if m[1] in wanted]
        else:
            wanted = set(parse_set(spec, [m[0] for m in messages]))
            chosen = [m for m in messages if m[0] in wanted]
        body_items = {'BODY[]', 'BODY.PEEK[]', 'RFC822', 'RFC822.HEADER', 'BODY.PEEK[HEADER]',
                      'BODY[HEADER]'}
        body = any(item in ('BODY[]', 'BODY.PEEK[]', 'RFC822') for item in items)
        self.log(command=command, mailbox=self.selected.name,
                 uids=[m[1] for m in chosen], body=body)
        flags = self.selected.flags()
        for seq, uid, path in chosen:
            data = load_message(path)
            parts = []
            literals = []
            for item in items:
                if item == 'UID':
                    parts.append(f'UID {uid}')
                elif item == 'FLAGS':
                    parts.append('FLAGS (' + ' '.join(flags.get(os.path.basename(path), [])) + ')')
                elif item == 'RFC822.SIZE':
                    parts.append(f'RFC822.SIZE {len(data)}')
                elif item == 'INTERNALDATE':
                    parts.append(f'INTERNALDATE "{internal_date(data, path)}"')
                elif item in body_items:
                    if item in ('RFC822.HEADER', 'BODY.PEEK[HEADER]', 'BODY[HEADER]'):
                        head, sep, _ = data.partition(b'\r\n\r\n')
                        content = head + sep
                        name = 'RFC822.HEADER' if item == 'RFC822.HEADER' else 'BODY[HEADER]'
                    else:
                        content = data
                        name = 'RFC822' if item == 'RFC822' else 'BODY[]'
                    parts.append(f'{name} {{{len(content)}}}')
                    literals.append((len(parts) - 1, content))
                else:
                    raise ValueError(f'unsupported fetch item {item}')
            self.write_fetch(seq, parts, literals)

    def write_fetch(self, seq, parts, literals):
        """`* <seq> FETCH (...)`, with each literal's bytes right after its `{n}`."""
        out = f'* {seq} FETCH ('.encode()
        literal_at = dict(literals)
        for i, part in enumerate(parts):
            if i:
                out += b' '
            out += part.encode()
            if i in literal_at:
                out += b'\r\n' + literal_at[i]
        out += b')\r\n'
        self.wfile.write(out)


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, address, store, user, password, log_path=None, ssl_context=None):
        self.store = store
        self.user = user
        self.password = password
        self.log_path = log_path
        self.log_lock = threading.Lock()
        self.ssl_context = ssl_context
        super().__init__(address, Session)

    def get_request(self):
        sock, address = super().get_request()
        if self.ssl_context is not None:
            sock = self.ssl_context.wrap_socket(
                sock, server_side=True, do_handshake_on_connect=False)
        return sock, address

    def handle_error(self, request, client_address):
        # A client that goes away or refuses the certificate is not the server's problem.
        if isinstance(sys.exc_info()[1], OSError):
            return
        super().handle_error(request, client_address)

    def log(self, entry):
        if not self.log_path:
            return
        with self.log_lock, open(self.log_path, 'a', encoding='utf-8') as f:
            f.write(json.dumps(entry) + '\n')


def make_self_signed_cert(directory):
    """A self-signed P-256 certificate for 127.0.0.1 and localhost, made with `openssl`:
    `(certificate path, key path)`. The certificate is its own trust anchor for the client."""
    cert = os.path.join(directory, 'imap-test-cert.pem')
    key = os.path.join(directory, 'imap-test-key.pem')
    subprocess.run(
        ['openssl', 'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1',
         '-nodes', '-keyout', key, '-out', cert, '-days', '2', '-subj', '/CN=localhost',
         '-addext', 'subjectAltName=IP:127.0.0.1,DNS:localhost',
         '-addext', 'basicConstraints=critical,CA:FALSE',
         '-addext', 'keyUsage=critical,digitalSignature',
         '-addext', 'extendedKeyUsage=serverAuth'],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return cert, key


def server_ssl_context(cert_path, key_path):
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert_path, key_path)
    return context


def start(root, host, port, user, password, log_path=None, ssl_context=None, special_use=True):
    """Starts a server on a background thread and returns it (`server.server_address` has the
    port; `server.shutdown()` stops it)."""
    server = Server((host, port), Store(root, special_use), user, password, log_path, ssl_context)
    thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': 0.05},
                              daemon=True)
    thread.start()
    return server


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    parser.add_argument('--root', required=True, help='the folder of mailboxes')
    parser.add_argument('--host', default='127.0.0.1')
    parser.add_argument('--port', type=int, default=0, help='0 picks a free port')
    parser.add_argument('--user', default='ada@example.org')
    parser.add_argument('--password', default=None,
                        help='default: the AZMAIL_TEST_PASSWORD environment variable')
    parser.add_argument('--log', default=None, help='JSON lines of the commands')
    parser.add_argument('--tls', action='store_true', help='implicit TLS with a generated cert')
    parser.add_argument('--no-special-use', action='store_true')
    args = parser.parse_args()
    password = args.password or os.environ.get('AZMAIL_TEST_PASSWORD')
    if not password:
        parser.error('give --password or set AZMAIL_TEST_PASSWORD')
    context = None
    cert_dir = None
    if args.tls:
        if not shutil.which('openssl'):
            parser.error('--tls needs the openssl tool')
        cert_dir = tempfile.mkdtemp(prefix='azmail-imap-cert-')
        cert, key = make_self_signed_cert(cert_dir)
        context = server_ssl_context(cert, key)
    server = start(args.root, args.host, args.port, args.user, password, args.log, context,
                   not args.no_special_use)
    print(f'IMAP_SERVER_PORT {server.server_address[1]}', flush=True)
    if args.tls:
        print(f'IMAP_SERVER_CA {cert}', flush=True)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass
    finally:
        server.shutdown()
        server.server_close()
        if cert_dir:
            shutil.rmtree(cert_dir, ignore_errors=True)


if __name__ == '__main__':
    main()
