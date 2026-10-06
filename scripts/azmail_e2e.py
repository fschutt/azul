#!/usr/bin/env python3
"""AzMail end to end: add an account, receive, read, reply in a second window, send, find it in Sent.

1. starts the IMAP test server (examples/azul-mail/scripts/imap_server.py) over a copy of the
   sample mail, and SEND's SMTP sink (scripts/azmail_smtp_sink.py) on a free port;
2. starts AzMail headless (AZ_BACKEND=headless, the debug server on --debug-port) with an empty
   AZMAIL_DATA and the server's password in AZMAIL_TEST_PASSWORD (a headless run never touches
   the real keyring);
3. walks File > Add Account: name and address; the IMAP server (unencrypted, 127.0.0.1); sending
   through an SMTP server = the sink, STARTTLS off; Finish - and waits for the first
   Send / Receive (AZMAIL_SYNC_DONE);
4. checks the files: account.json with the name and no password, SEND's sending.json with the
   route, the synced Inbox;
5. opens "Re: Garden plan for October", clicks Reply: a SECOND WINDOW opens
   (AZMAIL_COMPOSE_OPEN <window id> reply); through the debug server addressed to that window
   it checks the To line, the subject and the quote, types a line at the caret (the top) and
   clicks Send;
6. waits for AZMAIL_SEND_DONE <window id> sent, then checks what the sink received (From with
   the name, To, Subject, In-Reply-To / References of the original, the typed line above the
   quote with "> " marks, an HTML part with a blockquote) and that the window closed, and that
   the mail is in Sent (mail/sent/index.jsonl, and the window's Sent Items);
7. New E-mail, a subject and a line, Save Draft: the draft is in mail/drafts; Discard closes;
8. the password is in no file AzMail wrote and in none of its output.

The submission phase (`--phase submission`, MAIL9 left 2) walks the Sending page's third choice:
the sink is a submission server (`--auth`: MAIL needs a sign-in); after the wizard the account's
own outgoing server (account.json `smtp`, which the wizard has no field for) is pointed at the sink
and AzMail restarts on File > Account Settings; Sending: "Through my provider's server (sign in)",
DKIM on, Create a key, Save; sending.json says `submission` with the key's public half; a new mail
is sent, and the sink got it signed in as the account (PLAIN / LOGIN on the unencrypted test
connection) with a DKIM-Signature of the account's domain and selector.

Usage (from the azul repository, after building AzMail and a libazul with the debug server):

    python3 scripts/azmail_e2e.py [--bin target/release/AzMail] [--debug-port 8772]
        [--timeout 150] [--runner <run_capped.sh>] [--keep-logs]

With --runner the app runs under that script (`<runner> --cap-mb 1500 --seconds <timeout>
--log <file> -- env ... AzMail`), which the house rules ask for on this machine. AZMAIL_BIN also
names the binary. Logs and data go to a temporary folder that is printed at the end (kept on
failure, or always with --keep-logs).
"""
import argparse
import email
import email.policy
import json
import os
import re
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

import azlin_e2e

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..'))
MAIL_SCRIPTS = os.path.join(REPO, 'examples', 'azul-mail', 'scripts')

USER = 'ada@example.org'
NAME = 'Ada Lovelace'
ACCOUNT_ID = 'ada@example.org'
REPLY_TO = 'Re: Garden plan for October'
ORIGINAL_ID = '<plain-reply-1@example.org>'
TYPED = 'Thanks Ben, Saturday it is!'


def log(line):
    print(f'[azmail-e2e] {line}', flush=True)


class Failure(Exception):
    pass


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
        for parts in (('release',), ('consumer', 'release'), ('debug',), ('consumer', 'debug')):
            candidates.append(os.path.join(root, 'target', *parts, exe))
    for c in candidates:
        if c and os.path.isfile(c):
            return os.path.abspath(c)
    raise SystemExit('the AzMail binary was not found (pass --bin); tried:\n  ' +
                     '\n  '.join(c for c in candidates if c))


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)


class Run:
    def __init__(self, args):
        self.args = args
        self.deadline = time.time() + args.timeout
        self.tmp = tempfile.mkdtemp(prefix='azmail-e2e-')
        self.server_root = os.path.join(self.tmp, 'server-mail')
        self.data = os.path.join(self.tmp, 'data')
        self.sink_dir = os.path.join(self.tmp, 'sink')
        self.server_log = os.path.join(self.tmp, 'server.log')
        self.password = 'pw-' + secrets.token_urlsafe(18)
        self.children = []
        self.debug = args.debug_port
        # More switches for the SMTP sink (the submission phase makes it a submission server).
        self.sink_args = []

    # -- processes --

    def start(self, name, command, env):
        out = open(os.path.join(self.tmp, f'{name}.out'), 'w')
        err = open(os.path.join(self.tmp, f'{name}.err'), 'w')
        # A session of its own: stopping it stops the whole tree - with --runner the app is
        # the runner's child, and terminating only the runner left the app running.
        child = subprocess.Popen(command, env={**os.environ, **env}, stdout=out, stderr=err,
                                 stdin=subprocess.DEVNULL, start_new_session=True)
        self.children.append((name, child))
        return child

    @staticmethod
    def stop_tree(child, sig=signal.SIGTERM):
        try:
            os.killpg(child.pid, sig)
        except (ProcessLookupError, PermissionError):
            pass

    def stop(self, child):
        if child.poll() is None:
            self.stop_tree(child)
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.stop_tree(child, signal.SIGKILL)
            child.wait(timeout=5)

    def stop_all(self):
        for _, child in self.children:
            self.stop(child)

    def runner_log(self, name):
        """With --runner, the app's own output goes to the runner's --log file."""
        return os.path.join(self.tmp, f'runner-{name}.log')

    def output(self, name, stream='out'):
        text = ''
        paths = [os.path.join(self.tmp, f'{name}.{stream}')]
        if self.args.runner:
            paths.append(self.runner_log(name))
        for path in paths:
            try:
                with open(path, encoding='utf-8', errors='replace') as f:
                    text += f.read()
            except OSError:
                pass
        return text

    def until(self, what, check, interval=0.25, limit=None):
        """Waits for `check`; `limit` seconds at most (a check that waits for one feature must
        not eat the whole run's time), else until the run's deadline."""
        last = None
        deadline = self.deadline if limit is None else min(self.deadline, time.time() + limit)
        while time.time() < deadline:
            for name, child in self.children:
                if name == 'azmail' and child.poll() is not None:
                    raise Failure(f'AzMail exited ({child.returncode}) while waiting for {what}')
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, KeyError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure(f'timed out waiting for {what}' + (f' (last error: {last})' if last else ''))

    def printed(self, key, pattern=r'.+'):
        """Every `KEY value` line AzMail printed, the values."""
        return re.findall(rf'^{re.escape(key)} ({pattern})$', self.output('azmail'), re.M)

    # -- the debug server --

    def op(self, op, window=None, **params):
        body = {'op': op}
        body.update(params)
        if window:
            body['window_id'] = window
        request = urllib.request.Request(f'http://127.0.0.1:{self.debug}/',
                                         data=json.dumps(body).encode(), method='POST')
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.loads(response.read().decode('utf-8') or '{}')

    def must(self, op, window=None, **params):
        answer = self.op(op, window, **params)
        if isinstance(answer, dict) and answer.get('status') == 'error':
            raise Failure(f'{op} {json.dumps(params)} (window {window}) failed: '
                          f'{json.dumps(answer)[:300]}')
        return answer

    def frame(self, window=None, n=1):
        for _ in range(n):
            self.must('wait_frame', window)

    def texts(self, window=None):
        return list(strings(self.op('get_node_hierarchy', window)))

    def shows(self, text, window=None):
        return any(text in t for t in self.texts(window))

    def settle(self, window=None, limit=2.0):
        """Waits for the window's animations: a click lands where a node is PAINTED, and the
        wizard's pages slide in (azlin_e2e.settle_animations, the settle of 05ef3a8f4)."""
        def animations():
            answer = self.must('get_animations', window)
            data = answer.get('data') if isinstance(answer, dict) else None
            return data.get('value') if isinstance(data, dict) and 'value' in data else data
        azlin_e2e.settle_animations(animations, lambda: self.frame(window), limit)

    def click(self, text, window=None, closes=False):
        """Clicks `text` in `window`; `closes`: the click closes that window, so the frame is
        the main window's (a closed window answers no frame)."""
        self.settle(window)
        self.must('click', window, text=text)
        self.frame(None if closes else window)

    def click_id(self, dom_id, window=None):
        self.settle(window)
        self.must('click', window, selector=f'#{dom_id}')
        self.frame(window)

    def node_with_text(self, text, window=None, within=None):
        """The index of the first node whose text is exactly `text` (in a 1-tuple: index 0 is
        a node too) - inside a node of the class `within` if given - else None.
        `click(text=...)` takes the first node CONTAINING the text, and a note can hold a
        button's word ("Save puts it into the system keyring", "Finish adds the account"); a
        wizard's step names its last step "Finish" like its button."""
        answer = self.op('get_node_hierarchy', window)
        nodes = (((answer or {}).get('data') or {}).get('value') or {}).get('nodes') or []
        by_index = {n.get('index'): n for n in nodes}

        def inside(node):
            for _ in range(256):
                if node is None:
                    return False
                if within in (node.get('classes') or []):
                    return True
                node = by_index.get(node.get('parent'))
            return False

        for n in nodes:
            if (n.get('text') or '').strip() == text and n.get('index') is not None:
                if within and not inside(n):
                    continue
                # A text node has no box of its own: its nearest ancestor that has one.
                target = n
                while target is not None and not target.get('rect'):
                    target = by_index.get(target.get('parent'))
                return ((target or n)['index'],)
        return None

    def click_exact(self, text, window=None, within=None):
        found = self.until(f'a node reading exactly "{text}"',
                           lambda: self.node_with_text(text, window, within), limit=20)
        self.settle(window)
        self.must('click', window, node_id=found[0])
        self.frame(window)

    def type_into(self, dom_id, text, window=None):
        # A redraw can follow a field's focus change: focus, settle, focus again, then type.
        self.must('focus_node', window, selector=f'#{dom_id}')
        self.frame(window)
        self.must('focus_node', window, selector=f'#{dom_id}')
        self.must('text_input', window, text=text)
        self.frame(window, 2)

    def key(self, key, window=None, primary=False):
        """A tap of `key` (with the platform's shortcut modifier: Cmd on macOS, Ctrl
        elsewhere); the key_up releases every modifier (the E2E key_up rule)."""
        mods = {'shift': False, 'ctrl': False, 'alt': False, 'meta': False}
        if primary:
            mods['meta' if sys.platform == 'darwin' else 'ctrl'] = True
        self.must('key_down', window, key=key, modifiers=mods)
        self.must('key_up', window, key=key,
                  modifiers={'shift': False, 'ctrl': False, 'alt': False, 'meta': False})
        self.frame(window)

    def replace_in(self, dom_id, text, window=None):
        """Types `text` over what a field holds - a pre-filled default (the Sending page's
        port says 587): focus, select all, type."""
        self.must('focus_node', window, selector=f'#{dom_id}')
        self.frame(window)
        self.must('focus_node', window, selector=f'#{dom_id}')
        self.key('a', window, primary=True)
        self.must('text_input', window, text=text)
        self.frame(window, 2)

    def focused_selector(self, window=None):
        answer = self.must('get_focus_state', window, seat=0)
        data = answer.get('data') if isinstance(answer, dict) else None
        value = data.get('value') if isinstance(data, dict) and 'value' in data else data
        node = (value or {}).get('focused_node') or {}
        return node.get('selector') or ''

    # -- the steps --

    def start_servers(self):
        shutil.copytree(os.path.join(MAIL_SCRIPTS, 'sample_mail'), self.server_root)
        os.makedirs(self.data)
        os.makedirs(self.sink_dir)
        self.start('server', [sys.executable, os.path.join(MAIL_SCRIPTS, 'imap_server.py'),
                              '--root', self.server_root, '--port', '0', '--user', USER,
                              '--log', self.server_log],
                   {'AZMAIL_TEST_PASSWORD': self.password})
        self.imap_port = int(self.until('the IMAP server', lambda: (re.search(
            r'^IMAP_SERVER_PORT (\d+)$', self.output('server'), re.M) or [None, None])[1]))
        self.start('sink', [sys.executable, os.path.join(HERE, 'azmail_smtp_sink.py'), '0',
                            self.sink_dir] + self.sink_args, {})
        self.smtp_port = int(self.until('the SMTP sink', lambda: (re.search(
            r'^AZMAIL_SINK_READY (\d+)$', self.output('sink'), re.M) or [None, None])[1]))
        log(f'IMAP 127.0.0.1:{self.imap_port}, SMTP sink 127.0.0.1:{self.smtp_port}')

    def start_app(self, *app_args):
        binary = find_binary(self.args.bin)
        log(f'AzMail: {binary} {" ".join(app_args)}'.rstrip())
        env = {
            'AZ_BACKEND': 'headless',
            'AZ_DEBUG': str(self.debug),
            'AZMAIL_DATA': self.data,
            'AZMAIL_TEST_PASSWORD': self.password,
        }
        command = [binary, *app_args]
        if self.args.runner:
            command = [self.args.runner, '--cap-mb', '1500', '--seconds',
                       str(int(self.args.timeout) + 30), '--log',
                       self.runner_log('azmail'), '--', 'env'] + \
                      [f'{k}={v}' for k, v in env.items()] + [binary, *app_args]
        self.start('azmail', command, env)

    def add_account(self):
        self.until('the Add Account wizard', lambda: self.shows('Add Account'))
        self.type_into('__azmail_acct_name', NAME)
        self.type_into('__azmail_acct_email', USER)
        self.click('Next >')
        self.until('the incoming server page', lambda: self.shows('Incoming mail server'))
        self.type_into('__azmail_acct_imap_host', '127.0.0.1')
        self.type_into('__azmail_acct_imap_port', str(self.imap_port))
        self.click('Unencrypted connection')
        self.click('Next >')
        self.until('the sending page', lambda: self.shows('Send mail:'))
        self.click('Through an SMTP server')
        self.until('the SMTP fields', lambda: self.shows('Outgoing mail server'))
        self.type_into('__azmail_send_host', '127.0.0.1')
        # The port field is pre-filled with the submission port (587): typed over, not after.
        self.replace_in('__azmail_send_port', str(self.smtp_port))
        self.click('Use STARTTLS when the server offers it')
        self.click('Next >')
        self.until('the last page', lambda: self.shows('Finish adds the account'))
        # The page's own text says "Finish adds the account ..." and the wizard's last step is
        # named "Finish" too: the button in the wizard's button row.
        self.click_exact('Finish', within='__azul-native-wizard-layout-buttons')
        saved = self.until('AZMAIL_ACCOUNT_SAVED', lambda: self.printed('AZMAIL_ACCOUNT_SAVED'))
        log(f'account saved: {saved[0]}')
        done = self.until('the first Send/Receive', lambda: self.printed('AZMAIL_SYNC_DONE') or
                          self.printed('AZMAIL_SYNC_FAILED'))
        if self.printed('AZMAIL_SYNC_FAILED'):
            raise Failure(f'the first Send/Receive failed: {self.printed("AZMAIL_SYNC_FAILED")}')
        log(f'first Send/Receive: {done[0]}')

    def check_account_files(self):
        root = os.path.join(self.data, ACCOUNT_ID)
        with open(os.path.join(root, 'account.json'), encoding='utf-8') as f:
            account = json.load(f)
        if account.get('email') != USER or account.get('name') != NAME:
            raise Failure(f'account.json is {account}')
        with open(os.path.join(root, 'sending.json'), encoding='utf-8') as f:
            sending = json.load(f)
        route = sending.get('route') or {}
        if route.get('kind') != 'smtp' or route.get('port') != self.smtp_port \
                or route.get('host') != '127.0.0.1' or sending.get('tls') != 'off':
            raise Failure(f'sending.json is {sending}')
        with open(os.path.join(root, 'mail', 'inbox', 'index.jsonl'), encoding='utf-8') as f:
            inbox = [json.loads(line) for line in f if line.strip()]
        if REPLY_TO not in [e['subject'] for e in inbox]:
            raise Failure(f'the synced inbox is {[e["subject"] for e in inbox]}')
        log(f'account.json, sending.json (SMTP 127.0.0.1:{self.smtp_port}, STARTTLS off), '
            f'{len(inbox)} messages in the inbox')

    def check_main_window(self):
        for text in ('Inbox', 'Sent Items', 'Junk E-mail', 'Arrange By:', REPLY_TO):
            self.until(f'the main window to show "{text}"', lambda t=text: self.shows(t))
        log('the main window shows the folders and the messages')

    def reply(self):
        self.click(REPLY_TO)
        self.until('the reading pane', lambda: self.shows('I can bring the tulip bulbs'))
        self.click('Reply')
        opened = self.until('the compose window', lambda: self.printed(
            'AZMAIL_COMPOSE_OPEN', r'\S+ reply'))
        window = opened[-1].split()[0]
        log(f'the reply opened in its own window: {window}')
        self.until('the compose window over the debug server',
                   lambda: self.shows('wrote:', window))
        texts = self.texts(window)
        for want in ('Ben Okafor <ben@example.org>', REPLY_TO,
                     'Shall we plant the bulbs before the first frost?'):
            if not any(want in t for t in texts):
                raise Failure(f'the reply window does not show {want!r}')
        # The caret is in the editor, at the top (above the quote).
        self.until('the editor to take the focus',
                   lambda: '__azmail_compose_body' in self.focused_selector(window) or self._focus_editor(window))
        self.must('text_input', window, text=TYPED)
        self.frame(window, 2)
        # Typing in the rich-text editor does not rebuild its DOM (the engine edits, the editor
        # follows: RichTextEditor's "Path 2"), so the window's node texts never show the line;
        # the caret does - past the typed line, in the first block. The mail the sink gets
        # carries the line itself (check_sink).
        self.until('the typed line in the editor', lambda: self.caret(window) == (
            'p#__azmail_compose_body-0.__azul-rte-block', len(TYPED.encode('utf-8'))))
        self.click_id('__azmail_compose_send', window)
        sent = self.until('the send', lambda: [line for line in self.printed('AZMAIL_SEND_DONE')
                                               if line.startswith(window + ' ')])
        verdict = sent[-1].split(' ', 2)
        if verdict[1] != 'sent':
            raise Failure(f'the mail was not sent: {sent[-1]}')
        log(f'sent: {sent[-1]}')
        self.until('the compose window to close', lambda: window in self.printed(
            'AZMAIL_COMPOSE_CLOSED'))
        return window

    def _focus_editor(self, window):
        self.must('focus_node', window, selector='#__azmail_compose_body')
        self.frame(window)
        return False

    def other_window(self, known):
        """The id of an open window not in `known` (a Modal's own window), else None."""
        answer = self.must('list_windows')
        data = answer.get('data') if isinstance(answer, dict) else None
        value = data.get('value') if isinstance(data, dict) and 'value' in data else data
        ids = [w.get('window_id') for w in (value or {}).get('windows') or []]
        others = [i for i in ids if i not in known]
        return others[0] if others else None

    def caret(self, window=None):
        """(the block the caret is in - its selector -, the caret's byte in it), else None."""
        answer = self.must('get_selection_state', window)
        value = ((answer.get('data') or {}).get('value') or {}) if isinstance(answer, dict) else {}
        for sel in value.get('selections') or []:
            for r in sel.get('ranges') or []:
                if r.get('selection_type') == 'cursor':
                    return sel.get('selector') or '', r.get('cursor_position')
        return None

    def check_sink(self):
        path = os.path.join(self.sink_dir, '0001.eml')
        self.until('the sink to store the mail', lambda: os.path.isfile(path))
        with open(path, 'rb') as f:
            raw = f.read()
        msg = email.message_from_bytes(raw, policy=email.policy.default)
        if msg['From'] != f'{NAME} <{USER}>':
            raise Failure(f'From is {msg["From"]!r}')
        if 'ben@example.org' not in (msg['To'] or ''):
            raise Failure(f'To is {msg["To"]!r}')
        if msg['Subject'] != REPLY_TO:
            raise Failure(f'Subject is {msg["Subject"]!r}')
        if (msg['In-Reply-To'] or '').strip() != ORIGINAL_ID:
            raise Failure(f'In-Reply-To is {msg["In-Reply-To"]!r}')
        if ORIGINAL_ID not in (msg['References'] or ''):
            raise Failure(f'References is {msg["References"]!r}')
        if msg['Bcc'] is not None:
            raise Failure('a Bcc header went out')
        text = msg.get_body(preferencelist=('plain',)).get_content()
        html = msg.get_body(preferencelist=('html',))
        if TYPED not in text:
            raise Failure(f'the typed line is not in the text part:\n{text}')
        if '> sounds good. I can bring the tulip bulbs on Saturday.' not in text:
            raise Failure(f'the quote is not in the text part:\n{text}')
        if text.index(TYPED) > text.index('wrote:'):
            raise Failure(f'the typed line is not above the quote:\n{text}')
        if html is None or '<blockquote type="cite">' not in html.get_content():
            raise Failure('the HTML part has no quote')
        with open(os.path.join(self.sink_dir, '0001.json'), encoding='utf-8') as f:
            envelope = json.load(f)
        if envelope.get('rcpt_to') != ['ben@example.org']:
            raise Failure(f'the envelope recipients are {envelope.get("rcpt_to")}')
        log('the sink got the reply: From, To, Subject, In-Reply-To, References, the typed '
            'line above the "> " quote, an HTML part with a blockquote')

    def check_sent(self):
        path = os.path.join(self.data, ACCOUNT_ID, 'mail', 'sent', 'index.jsonl')
        def filed():
            with open(path, encoding='utf-8') as f:
                return [json.loads(line) for line in f if line.strip()]
        entries = self.until('the mail in mail/sent', lambda: [e for e in filed()
                                                               if e['subject'] == REPLY_TO])
        log(f'filed in Sent: {entries[0]["path"]}')
        self.click('Sent Items')
        self.until('Sent Items to list the reply', lambda: self.shows('To: Ben Okafor'))
        log('the window lists it in Sent Items')

    def draft(self):
        self.click('New E-mail')
        opened = self.until('a new compose window', lambda: self.printed(
            'AZMAIL_COMPOSE_OPEN', r'\S+ new'))
        window = opened[-1].split()[0]
        self.until('the new window', lambda: self.shows('Untitled - Message (HTML)', window))
        self.type_into('__azmail_compose_to', 'cleo@example.org', window)
        self.type_into('__azmail_compose_subject', 'Bulb order', window)
        self.click('Save Draft', window)
        saved = self.until('the draft', lambda: [line for line in self.printed('AZMAIL_DRAFT_SAVED')
                                                 if line.startswith(window + ' ')])
        log(f'draft saved: {saved[-1]}')
        path = os.path.join(self.data, ACCOUNT_ID, 'mail', 'drafts', 'index.jsonl')
        with open(path, encoding='utf-8') as f:
            drafts = [json.loads(line) for line in f if line.strip()]
        if [d['subject'] for d in drafts] != ['Bulb order']:
            raise Failure(f'mail/drafts holds {drafts}')
        self.click('Discard', window, closes=True)
        self.until('the draft window to close', lambda: window in self.printed(
            'AZMAIL_COMPOSE_CLOSED'))
        log('a new mail saved as a draft in mail/drafts, then discarded')

    def check_secret(self):
        secret = self.password.encode()
        for dirpath, _, files in os.walk(self.data):
            for name in files:
                with open(os.path.join(dirpath, name), 'rb') as f:
                    if secret in f.read():
                        raise Failure(f'the password is in {os.path.join(dirpath, name)}')
        if secret in (self.output('azmail') + self.output('azmail', 'err')).encode():
            raise Failure("the password is in AzMail's output")
        log('the password is in no file and no output')

    def run(self):
        log(f'logs and data: {self.tmp}')
        self.start_servers()
        self.start_app()
        self.add_account()
        self.check_account_files()
        self.check_main_window()
        self.reply()
        self.check_sink()
        self.check_sent()
        self.draft()
        self.check_secret()


# ---- the sample phase (MAIL6): the look and the app-kit flows, no servers ----

# AzMail's DOM ids carry the app's prefix (ui ids module, `__azmail_`).
PREFIX = '__azmail_'
MAIN_SIZE = (1280, 860)
COMPOSE_SIZE = (880, 700)
NEWSLETTER = 'Garden Weekly: bulbs, frost and a sale'
TASK = 'Order more tulip bulbs'


class SampleRun(Run):
    """AzMail --sample: every check runs, the failures are listed at the end (some wait for
    engine fixes of other wave-6 tasks: the report names them)."""

    def __init__(self, args):
        super().__init__(args)
        self.failures = []

    def check(self, what, ok, detail=''):
        if ok:
            log(f'ok: {what}')
        else:
            log(f'FAILED: {what} {detail}')
            self.failures.append(f'{what} {detail}')

    def start_sample_app(self, name='azmail'):
        binary = find_binary(self.args.bin)
        env = {
            'AZ_BACKEND': 'headless',
            'AZ_DEBUG': str(self.debug),
            'AZMAIL_DATA': os.path.join(self.data, 'AzMail'),
            'AZLIN_DATA': self.data,
        }
        command = [binary, '--sample', '--size', f'{MAIN_SIZE[0]}x{MAIN_SIZE[1]}', '--mode',
                   'light']
        if self.args.runner:
            command = [self.args.runner, '--cap-mb', '1500', '--seconds',
                       str(int(self.args.timeout) + 30), '--log',
                       self.runner_log(name), '--', 'env'] + \
                      [f'{k}={v}' for k, v in env.items()] + command
        self.start(name, command, env)
        self.until('the sample Inbox', lambda: self.shows(NEWSLETTER))

    def layout(self, selector, window=None):
        answer = self.must('get_node_layout', window, selector=selector)
        data = answer.get('data') if isinstance(answer, dict) else None
        value = data.get('value') if isinstance(data, dict) and 'value' in data else data
        return (value or {}).get('rect') or {}

    def bottom(self, selector, window=None):
        rect = self.layout(selector, window)
        return rect.get('y', 0) + rect.get('height', 0)

    def widths(self, cls, window=None):
        answer = self.op('get_node_hierarchy', window)
        nodes = (((answer or {}).get('data') or {}).get('value') or {}).get('nodes') or []
        return [round((n.get('rect') or {}).get('width', -1)) for n in nodes
                if cls in (n.get('classes') or [])]

    def check_fills(self, window=None, size=MAIN_SIZE, what='the main window'):
        bottom = self.bottom('.__azul-native-office-shell-status', window)
        self.check(f'{what}: the status bar sits at the window\'s bottom edge',
                   abs(bottom - size[1]) <= 1.0, f'(its bottom is {bottom}, the window {size[1]})')

    def check_prefixed_ids(self):
        answer = self.op('get_node_hierarchy')
        nodes = (((answer or {}).get('data') or {}).get('value') or {}).get('nodes') or []
        app_ids = [n['id'] for n in nodes if n.get('id') and not n['id'].startswith('__azul')
                   and not n['id'].startswith(('shell-', 'appkit-'))]
        bad = [i for i in app_ids if not i.startswith(PREFIX)]
        self.check('every id AzMail sets carries the __azmail_ prefix', not bad, f'{bad}')

    def check_open_newsletter(self):
        self.click(NEWSLETTER)
        self.until('the newsletter in the reading pane', lambda: self.printed('AZMAIL_OPEN'))
        self.frame(None, 3)
        lists = self.widths('__azul-native-summary-list')
        panes = self.widths('__azul-native-reading-pane')
        # RED until the engine fix (MAILENG6): the split's panes collapse to 0 px after the
        # HTML mail with its table is opened (a resize lays them out right).
        self.check('the message list and the reading pane keep their widths with an HTML mail open',
                   lists and panes and min(lists) > 100 and min(panes) > 100,
                   f'(list {lists}, reading pane {panes})')
        # The newsletter's 600 px table stays on its paper: the paper grows with the mail
        # (html.rs) and the pane scrolls it sideways.
        papers = self.widths('__azmail_paper')
        self.check('the newsletter\'s 600 px table stays on its paper',
                   papers and min(papers) >= 600, f'(paper {papers})')

    def todo_task(self):
        self.must('focus_node', selector='.__azul-native-todo-bar-task-input '
                                         '.__azul-native-text-input-container')
        self.frame()
        self.must('text_input', text=TASK)
        self.frame(None, 2)
        self.must('key_down', key='enter', modifiers={})
        self.must('key_up', key='enter', modifiers={})
        self.frame(None, 2)
        try:
            self.until('the To-Do bar to list the task', lambda: self.shows(TASK), limit=20)
        except Failure as e:
            self.check('the To-Do bar lists a typed task', False, str(e))
            return
        files = []

        def stored():
            files.clear()
            for dirpath, _, names in os.walk(os.path.join(self.data, 'tasks')):
                for n in names:
                    if n.endswith('.json') and not n.startswith('.'):
                        with open(os.path.join(dirpath, n), encoding='utf-8') as f:
                            if TASK in f.read():
                                files.append(os.path.join(dirpath, n))
            return files
        try:
            self.until('the task file in the shared task store', stored, limit=20)
            self.check('a To-Do bar task is a file of the shared task store (tasks/<list>/<id>.json)',
                       True, files[0])
        except Failure as e:
            self.check('a To-Do bar task is a file of the shared task store', False, str(e))

    def compose_window(self):
        self.click('New E-mail')
        opened = self.until('a compose window', lambda: self.printed('AZMAIL_COMPOSE_OPEN',
                                                                      r'\S+ new'))
        window = opened[-1].split()[0]
        self.until('the compose window', lambda: self.shows('Untitled - Message (HTML)', window))
        self.frame(window, 2)
        self.check_fills(window, COMPOSE_SIZE, 'the compose window')
        return window

    def close_guard(self, window):
        try:
            self.type_into(PREFIX + 'compose_subject', 'Bulb order', window)
            # The window's close (the title bar's button, Alt+F4): an edited mail is held and
            # asked about (CloseRequested + prevent_window_close, the CloseGuard widget).
            self.must('close', window)
            self.frame(None, 2)
            self.until('the "save changes?" question',
                       lambda: self.shows('Do you want to save changes', window), limit=20)
            asked = window not in self.printed('AZMAIL_COMPOSE_CLOSED')
            self.check('closing an edited mail asks "save changes?" and keeps the window', asked)
            # The question is a Modal: a window of its own over the compose window (the
            # CloseGuard's MessageBox in a <transient-window>), where the click lands - its
            # nodes in the compose window's DOM have no box there. The answer closes both.
            question = self.until('the question\'s window', lambda: self.other_window(
                ('azmail-main', window)), limit=20)
            self.click("Don't Save", question, closes=True)
            self.until('the compose window to close', lambda: window in self.printed(
                'AZMAIL_COMPOSE_CLOSED'), limit=20)
            self.check('"Don\'t Save" closes the window', True)
        except Failure as e:
            self.check('closing an edited mail asks "save changes?"', False, str(e))

    def zoom_in(self):
        # The status bar's + sits between the slider's track and the percent label: the
        # reading pane's zoom, 100 % -> 110 %, remembered in settings.json.
        try:
            track = self.layout('.__azul-native-statusbar-zoom-track')
            label = self.layout('.__azul-native-statusbar-zoom-label')
        except Failure as e:
            self.check('the status bar has the zoom', False, str(e))
            return
        if not track or not label:
            self.check('the status bar has the zoom', False, f'(track {track}, label {label})')
            return
        self.check('the status bar shows the zoom at 100 %', self.shows('100%'))
        x = (track['x'] + track['width'] + label['x']) / 2
        y = label['y'] + label['height'] / 2
        self.must('click', x=x, y=y)
        self.frame(None, 2)
        self.check("the status bar's + zooms the reading pane to 110 %", self.shows('110%'))

    def restart_keeps_tasks(self):
        for name, child in self.children:
            if name == 'azmail':
                self.stop(child)
        self.children = [(n, c) for n, c in self.children if n != 'azmail']
        # The next run's output starts empty (its markers are its own).
        for path in (os.path.join(self.tmp, 'azmail.out'), self.runner_log('azmail')):
            if os.path.exists(path):
                os.replace(path, path + '.1')
        self.start_sample_app('azmail')
        self.check('the To-Do bar task is there again after a restart', self.shows(TASK))

    def run(self):
        log(f'logs and data: {self.tmp}')
        os.makedirs(self.data)
        self.start_sample_app()
        self.check_fills()
        self.check('the navigation pane shows its module buttons', self.shows('Calendar'))
        self.check_prefixed_ids()
        self.todo_task()
        window = self.compose_window()
        self.close_guard(window)
        self.check_open_newsletter()
        self.zoom_in()
        self.restart_keeps_tasks()
        self.check('the zoom is remembered across a restart', self.shows('110%'))
        if self.failures:
            raise Failure(f'{len(self.failures)} check(s) failed: ' + '; '.join(self.failures))


# ---- the submission phase (MAIL9 left 2): the Sending page's third choice and DKIM ----

SUBMISSION_TO = 'ben@example.org'
SUBMISSION_SUBJECT = 'The bulbs are in'
SUBMISSION_LINE = 'The tulip bulbs came today.'


class SubmissionRun(Run):
    """Account Settings > Sending: "Through my provider's server (sign in)" and DKIM with a key
    made in the editor, Save, then a new mail: the sink (a submission server) takes it only
    after a sign-in, and it carries a DKIM-Signature of the account's domain."""

    def __init__(self, args):
        super().__init__(args)
        # A submission server: MAIL needs a sign-in with the account's user name (the address)
        # and its password. Without a certificate the sink offers AUTH on the plain connection,
        # which AzMail uses only for a server on this computer with STARTTLS off.
        self.sink_args = ['--auth', f'{USER}={self.password}']

    def stop_app(self):
        """Stops AzMail; its output so far is kept aside (`*.1`), the next run's is its own."""
        for name, child in self.children:
            if name == 'azmail':
                self.stop(child)
        self.children = [(n, c) for n, c in self.children if n != 'azmail']
        for path in (os.path.join(self.tmp, 'azmail.out'), os.path.join(self.tmp, 'azmail.err'),
                     self.runner_log('azmail')):
            if os.path.exists(path):
                os.replace(path, path + '.1')

    def output_all(self):
        """Everything AzMail printed, both runs."""
        text = ''
        for path in (os.path.join(self.tmp, 'azmail.out.1'), os.path.join(self.tmp, 'azmail.err.1'),
                     self.runner_log('azmail') + '.1'):
            try:
                with open(path, encoding='utf-8', errors='replace') as f:
                    text += f.read()
            except OSError:
                pass
        return text + self.output('azmail') + self.output('azmail', 'err')

    def point_outgoing_server_at_sink(self):
        """The account's own outgoing server - account.json `smtp`, which the wizard fills from
        the address (smtp.example.org:465) and has no field for - becomes the sink: AzMail is
        stopped, the file changed, AzMail started again on File > Account Settings."""
        self.stop_app()
        path = os.path.join(self.data, ACCOUNT_ID, 'account.json')
        with open(path, encoding='utf-8') as f:
            account = json.load(f)
        account['smtp'] = {'host': '127.0.0.1', 'port': self.smtp_port}
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(account, f, indent=2)
            f.write('\n')
        log(f'account.json: the outgoing server is the sink, 127.0.0.1:{self.smtp_port}')
        self.start_app('--screen', 'settings')

    def choose_submission_and_dkim(self):
        self.until('File > Account Settings', lambda: self.node_with_text('Sending'))
        self.click_exact('Sending')
        self.until('the Sending section', lambda: self.shows('Send mail:'))
        self.click("Through my provider's server (sign in)")
        # The note names the account's own server: the patched account.json is the one in use.
        self.until('the sign-in note naming the sink',
                   lambda: self.shows(f'AzMail signs in to 127.0.0.1 port {self.smtp_port}'))
        # STARTTLS stays off (the wizard turned it off): a sign-in without TLS only to a server
        # on this computer.
        self.click('Sign my mail with DKIM')
        self.until('the DKIM fields', lambda: self.shows('Domain and selector:'))
        self.click_id(PREFIX + 'dkim_create')
        self.until('the DKIM key', lambda: re.search(r'^AZMAIL_DKIM_KEY_MADE$',
                                                     self.output('azmail'), re.M), limit=60)
        self.until('the DNS record to publish',
                   lambda: self.shows("Publish this TXT record in your domain's DNS:"))
        self.click_exact('Save')
        saved = self.until('AZMAIL_ACCOUNT_SAVED', lambda: self.printed('AZMAIL_ACCOUNT_SAVED'))
        log(f'Account Settings saved: {saved[-1]}')

    def check_sending_file(self):
        path = os.path.join(self.data, ACCOUNT_ID, 'sending.json')
        with open(path, encoding='utf-8') as f:
            sending = json.load(f)
        route = sending.get('route') or {}
        dkim = sending.get('dkim') or {}
        if route.get('kind') != 'submission':
            raise Failure(f'sending.json route is {route}')
        if dkim.get('domain') != 'example.org' or not dkim.get('selector') \
                or not dkim.get('public_key'):
            raise Failure(f'sending.json dkim is {dkim}')
        if 'PRIVATE KEY' in json.dumps(sending):
            raise Failure('the DKIM private key is in sending.json')
        self.selector = dkim['selector']
        log(f'sending.json: route submission, DKIM d=example.org s={self.selector} '
            '(the public half only)')

    def send_new_mail(self):
        self.until('the main window', lambda: self.shows('New E-mail'))
        self.click('New E-mail')
        opened = self.until('a new compose window', lambda: self.printed(
            'AZMAIL_COMPOSE_OPEN', r'\S+ new'))
        window = opened[-1].split()[0]
        self.until('the new window', lambda: self.shows('Untitled - Message (HTML)', window))
        self.type_into(PREFIX + 'compose_to', SUBMISSION_TO, window)
        self.type_into(PREFIX + 'compose_subject', SUBMISSION_SUBJECT, window)
        self._focus_editor(window)
        self.must('text_input', window, text=SUBMISSION_LINE)
        self.frame(window, 2)
        self.until('the typed line in the editor', lambda: self.shows(SUBMISSION_LINE, window))
        self.click_id(PREFIX + 'compose_send', window)
        sent = self.until('the send', lambda: [line for line in self.printed('AZMAIL_SEND_DONE')
                                               if line.startswith(window + ' ')])
        if sent[-1].split(' ', 2)[1] != 'sent':
            raise Failure(f'the mail was not sent: {sent[-1]}')
        log(f'sent: {sent[-1]}')

    def check_signed_in_and_signed(self):
        eml = os.path.join(self.sink_dir, '0001.eml')
        self.until('the sink to store the mail', lambda: os.path.isfile(eml))
        with open(os.path.join(self.sink_dir, '0001.json'), encoding='utf-8') as f:
            envelope = json.load(f)
        if envelope.get('auth_user') != USER:
            raise Failure(f'the sink took the mail without the sign-in: {envelope}')
        if envelope.get('auth_mechanism') not in ('PLAIN', 'LOGIN'):
            raise Failure(f'signed in with {envelope.get("auth_mechanism")!r}, not a password')
        if envelope.get('rcpt_to') != [SUBMISSION_TO]:
            raise Failure(f'the envelope recipients are {envelope.get("rcpt_to")}')
        with open(eml, 'rb') as f:
            msg = email.message_from_bytes(f.read(), policy=email.policy.default)
        if msg['From'] != f'{NAME} <{USER}>' or msg['Subject'] != SUBMISSION_SUBJECT:
            raise Failure(f'From {msg["From"]!r}, Subject {msg["Subject"]!r}')
        signature = re.sub(r'\s+', '', str(msg['DKIM-Signature'] or ''))
        tags = dict(t.split('=', 1) for t in signature.split(';') if '=' in t)
        if tags.get('d') != 'example.org' or tags.get('s') != self.selector or not tags.get('b'):
            raise Failure(f'the DKIM-Signature is {msg["DKIM-Signature"]!r}')
        if SUBMISSION_LINE not in msg.get_body(preferencelist=('plain',)).get_content():
            raise Failure('the typed line is not in the text part')
        log(f'the sink got it signed in as {USER} ({envelope["auth_mechanism"]}) with a '
            f'DKIM-Signature d=example.org s={self.selector}')

    def check_secret(self):
        secret = self.password.encode()
        for dirpath, _, files in os.walk(self.data):
            for name in files:
                with open(os.path.join(dirpath, name), 'rb') as f:
                    if secret in f.read():
                        raise Failure(f'the password is in {os.path.join(dirpath, name)}')
        if secret in self.output_all().encode():
            raise Failure("the password is in AzMail's output")
        log('the password is in no file and no output (both runs)')

    def run(self):
        log(f'logs and data: {self.tmp}')
        self.start_servers()
        self.start_app()
        self.add_account()
        self.check_account_files()
        self.point_outgoing_server_at_sink()
        self.choose_submission_and_dkim()
        self.check_sending_file()
        self.send_new_mail()
        self.check_signed_in_and_signed()
        self.check_secret()


def run_phase(cls, args, what):
    run = cls(args)
    passed = False
    try:
        run.run()
        passed = True
        log(f'PASS: {what}')
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
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    parser.add_argument('--bin')
    parser.add_argument('--debug-port', type=int, default=8772)
    parser.add_argument('--timeout', type=float, default=150)
    parser.add_argument('--runner', help='run_capped.sh (caps the app\'s memory and time)')
    parser.add_argument('--keep-logs', action='store_true')
    parser.add_argument('--phase', choices=('all', 'sample', 'account', 'submission'),
                        default='all',
                        help='sample: --sample, the look and the app-kit flows (no servers); '
                             'account: the wizard, IMAP, SMTP; submission: Account Settings, '
                             'Sending: the signed-in route and DKIM, a mail to the sink')
    args = parser.parse_args()
    passed = True
    if args.phase in ('all', 'sample'):
        passed &= run_phase(SampleRun, args, 'sample: the window fills, ids, To-Do bar store, '
                                             'compose window, close guard, HTML mail, zoom, '
                                             'restart')
    if args.phase in ('all', 'account'):
        passed &= run_phase(Run, args, 'account, Send/Receive, reply window, send through SMTP, '
                                       'Sent, draft')
    if args.phase in ('all', 'submission'):
        passed &= run_phase(SubmissionRun, args, 'Sending: signed-in submission and DKIM, a mail '
                                                 'to the sink signed in and signed')
    sys.exit(0 if passed else 1)


if __name__ == '__main__':
    main()
