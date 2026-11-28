#!/usr/bin/env python3
"""azmail_sample_data.py OUT [--repo .]: the SAMPLE ACCOUNT of AzMail as files, for a phone.

AzMail's sample account is the kit flag `--sample` (examples/azul-mail/src/sample.rs: the test server's sample mail filed as if it had been
synced, `ada@example.org`, no network, no account of yours). A phone app has no command line and the phone shells have no `debug.az.args`, so on a
phone AzMail starts empty. This script writes the same files the flag would, in the formats of examples/azul-mail-core (account.rs, store.rs,
message.rs), under OUT/azmail-data/ada@example.org/, so that a phone started with AZMAIL_DATA=<that folder> shows the mailbox:

    azmail-data/ada@example.org/account.json
    azmail-data/ada@example.org/mail/<folder key>/index.jsonl | state.json | <yyyy>/<mm>/<uid>.eml

It reads the .eml files of examples/azul-mail/scripts/sample_mail/ (the folders INBOX, Sent, Spam, Entwürfe = Drafts, Work/Projects). The formats are
a copy of what the Rust code writes at the commit this was written against; a change there makes the phone show an empty mailbox (the app logs
`[azmail] left out <path>: <reason>` for a file it cannot read)."""
import argparse
import datetime
import email
import email.header
import email.policy
import email.utils
import json
import sys
from pathlib import Path

ADDRESS = "ada@example.org"
NAME = "Ada Lovelace"
# (folder key, the server's name, the name shown, the folder of sample_mail it is filled from or None)
FOLDERS = (
    ("inbox", "INBOX", "Inbox", "INBOX"),
    ("sent", "Sent", "Sent", "Sent"),
    ("spam", "Spam", "Spam", "Spam"),
    ("drafts", "Drafts", "Drafts", "Entwürfe"),
    ("Work", "Work", "Work", None),
    ("Work.Projects", "Work/Projects", "Work/Projects", "Work/Projects"),
)


def account_json():
    """account.json of version 1 (IMAP): the sample server on this computer, no TLS."""
    return json.dumps({
        "format": "azmail.account", "version": 1, "email": ADDRESS, "name": NAME, "username": ADDRESS,
        "imap": {"host": "localhost", "port": 1143}, "smtp": {"host": "localhost", "port": 2525},
        "security": "plain", "auth": "password",
    }, indent=2) + "\n"


def rfc3339_utc(seconds):
    return datetime.datetime.fromtimestamp(seconds, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def addresses(header_value):
    """`Name <addr>, addr2` of a From / To header, the names decoded."""
    out = []
    for name, addr in email.utils.getaddresses([header_value or ""]):
        name = str(email.header.make_header(email.header.decode_header(name))) if name else ""
        out.append(f"{name} <{addr}>" if name and addr else (addr or name))
    return ", ".join(out)


def entry_of(uid, raw, flags, path):
    """The index line of one message (examples/azul-mail-core/src/message.rs index_entry)."""
    msg = email.message_from_bytes(raw, policy=email.policy.default)
    when = None
    if msg["Date"]:
        try:
            when = email.utils.parsedate_to_datetime(str(msg["Date"]))
        except (TypeError, ValueError):
            when = None
    seconds = int(when.timestamp()) if when else 0
    return {
        "uid": uid, "message_id": str(msg["Message-ID"] or "").strip("<>"), "date": rfc3339_utc(seconds) if when else "",
        "from": addresses(str(msg["From"] or "")), "to": addresses(str(msg["To"] or "")), "subject": str(msg["Subject"] or ""),
        "flags": flags, "size": len(raw), "path": path,
    }, seconds


def message_key(folder, year, month, uid):
    return f"mail/{folder}/{year:04d}/{month:02d}/{uid}.eml"


def write(root, relative, data):
    target = root / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data if isinstance(data, bytes) else data.encode("utf-8"))


def build(repo, out):
    """Writes OUT/azmail-data/<address>/...; -> {folder key: number of messages}."""
    sample = Path(repo) / "examples" / "azul-mail" / "scripts" / "sample_mail"
    if not sample.is_dir():
        raise SystemExit(f"no sample mail in {sample}")
    account = Path(out) / "azmail-data" / ADDRESS
    counts = {}
    for key, server_name, display, source in FOLDERS:
        files = sorted((sample / source).glob("*.eml")) if source else []
        flags = [] if key == "inbox" else ["\\Seen"]
        index = []
        for uid, eml in enumerate(files, 1):
            raw = eml.read_bytes()
            seconds_entry, seconds = entry_of(uid, raw, flags, "")
            moment = datetime.datetime.fromtimestamp(seconds, datetime.timezone.utc) if seconds else None
            year, month = (moment.year, moment.month) if moment else (0, 0)
            path = message_key(key, year, month, uid)
            seconds_entry["path"] = path
            write(account, path, raw)
            index.append(seconds_entry)
        write(account, f"mail/{key}/index.jsonl", "".join(json.dumps(e, ensure_ascii=False, separators=(",", ":")) + "\n" for e in index))
        state = {"format": "azmail.folder", "version": 1, "server_name": server_name, "display": display, "uidvalidity": 1,
                 "last_uid": len(files), "messages": len(files), "synced_at": ""}
        write(account, f"mail/{key}/state.json", json.dumps(state, indent=2) + "\n")
        counts[key] = len(files)
    write(account, "account.json", account_json())
    return counts


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("out")
    ap.add_argument("--repo", default=".")
    args = ap.parse_args(argv)
    counts = build(args.repo, args.out)
    print("azmail sample account written to", Path(args.out) / "azmail-data" / ADDRESS, counts)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
