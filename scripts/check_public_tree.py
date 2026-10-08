#!/usr/bin/env python3
"""This repository is public: nothing secret may be committed to it.

Checks every tracked text file for credentials (private keys, JWTs, cloud and GitHub tokens, a
password or API key written into a file), and the scripts/ folder for agent notes that belong in
the private repository (prompts and progress logs: `scripts/*.PROGRESS.md`,
`scripts/*_YYYY_MM_DD.md`), apart from the design notes the code refers to (KEPT_NOTES, which
predate the rule). Exit code 1 lists every finding.

    python3 scripts/check_public_tree.py            the tracked tree
    python3 scripts/check_public_tree.py --self-test
"""
import re
import subprocess
import sys

# Credentials, as their issuers write them. A test that needs a key makes one at run time.
CREDENTIALS = [
    # a header with a key body under it (a header alone is how tests spell a secret-shaped file)
    (
        "private key",
        re.compile(
            r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----\s*\n"
            r"(?:[A-Za-z0-9+/=]{20,}\s*\n){2,}"
        ),
    ),
    ("JWT", re.compile(r"\beyJ[A-Za-z0-9_-]{16,}\.eyJ[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}")),
    # AWS's documentation keys (AKIAIOSFODNN7EXAMPLE) are the published SigV4 test vectors
    ("AWS access key id", re.compile(r"\b(?:AKIA|ASIA)(?![0-9A-Z]{9}EXAMPLE\b)[0-9A-Z]{16}\b")),
    ("GitHub token", re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36}\b|\bgithub_pat_[A-Za-z0-9_]{60,}")),
    ("Slack token", re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}")),
    ("Google API key", re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b")),
    ("Stripe key", re.compile(r"\b(?:sk|rk)_live_[0-9A-Za-z]{20,}")),
    (
        "a secret written into a file",
        re.compile(
            r"(?i)\b(?:password|passwd|secret(?:_key)?|api[_-]?key|access[_-]?token|auth[_-]?token)"
            r"\s*[:=]\s*[\"'](?![<$])[A-Za-z0-9+/_=.-]{24,}[\"']"
        ),
    ),
]

# Agent prompts and progress logs: the private repository keeps them (notes/azul-scripts/).
AGENT_NOTE = re.compile(r"^scripts/[^/]+(?:\.PROGRESS|_20[0-9]{2}_[0-9]{2}_[0-9]{2})\.md$")

# Design notes the code cites by path; they stay where the comments point.
KEPT_NOTES = {
    "scripts/B3_BUILDER_EXPORT.PROGRESS.md",
    "scripts/DEDUP_EDITORS_2026_10_02.md",
    "scripts/DRIVE1_AZDRIVE_2026_09_30.md",
    "scripts/GLOBAL_HOTKEYS_2026_09_28.md",
    "scripts/GLOBAL_HOTKEYS_DECLARATIVE_DESIGN_2026_09_28.md",
    "scripts/KEYS9_2026_10_03.md",
    "scripts/LAYOUTPERF8_2026_10_03.md",
    "scripts/MAILVIEW_2026_09_30.md",
    "scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md",
    "scripts/NOTIFICATIONS_RESEARCH_2026_09_28.md",
    "scripts/PDF9_2026_10_03.md",
    "scripts/R1_MAIL_RENDER_2026_09_30.md",
    "scripts/REFCI_2026_09_30.md",
    "scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md",
    "scripts/SYSUI8.PROGRESS.md",
    "scripts/SYSUI8_2026_10_03.md",
    "scripts/T1_APP_THEME_2026_09_29.md",
    "scripts/TABLE_A_2026_10_01.md",
    "scripts/TERM9_2026_10_03.md",
    "scripts/TEXT_SCROLL_VS_CARET_REVEAL_ARCHITECTURE_2026_09_26.md",
    "scripts/VIDEO_PATH_2026_09_30.md",
    "scripts/W1_INPUT_TYPES_2026_09_29.md",
}

SKIP_SUFFIXES = (".lock", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".ttf", ".otf", ".woff",
                 ".woff2", ".pdf", ".zip", ".gz", ".zst", ".wasm", ".bin", ".mp4", ".mp3", ".ogg")


def findings_in(path, text):
    out = []
    for name, pattern in CREDENTIALS:
        for m in pattern.finditer(text):
            line = text.count("\n", 0, m.start()) + 1
            out.append(f"{path}:{line}: {name}")
    return out


def note_finding(path):
    if AGENT_NOTE.match(path) and path not in KEPT_NOTES:
        return [f"{path}: an agent prompt or progress log (keep it in the private repository's notes/)"]
    return []


def tracked():
    out = subprocess.run(["git", "ls-files", "-z"], capture_output=True, check=True).stdout
    return [p for p in out.decode().split("\0") if p]


def check(paths, read):
    found = []
    for path in paths:
        found += note_finding(path)
        if path.endswith(SKIP_SUFFIXES):
            continue
        try:
            data = read(path)
        except OSError:
            continue
        if b"\0" in data[:8192] or len(data) > 4 * 1024 * 1024:
            continue
        found += findings_in(path, data.decode("utf-8", "replace"))
    return found


def self_test():
    fake = {
        "a.py": b"-----BEGIN PRIVATE KEY-----\n" + b"MIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n" * 3 + b"-----END PRIVATE KEY-----\n",
        "a2.py": b"KEY = '-----BEGIN PRIVATE KEY-----'  # a test's secret-shaped file\n",
        "e2.rs": b"const S3_ACCESS_KEY: &str = \"AKIAIOSFODNN7EXAMPLE\";\n",
        "b.md": b"token: eyJhbGciOiJFZERTQSJ9x.eyJhIjoicncifQxxxxxxxxx.c2lnbmF0dXJlc2lnbmF0dXJl\n",
        "c.rs": b"let password = \"hunter2hunter2hunter2hunter2\";\n",
        "d.rs": b"let password = \"<from the keyring>\"; // fine\n",
        "e.txt": b"AKIAABCDEFGHIJKLMNOP\n",
        "scripts/NEW_AGENT_2026_10_09.md": b"notes\n",
        "scripts/X.PROGRESS.md": b"notes\n",
        "scripts/run_e2e.py": b"print('ok')\n",
        "scripts/REFCI_2026_09_30.md": b"design\n",
    }
    got = check(list(fake), lambda p: fake[p])
    want = {"a.py", "b.md", "c.rs", "e.txt", "scripts/NEW_AGENT_2026_10_09.md", "scripts/X.PROGRESS.md"}
    hit = {f.split(":")[0] for f in got}
    assert hit == want, f"self-test: flagged {sorted(hit)}, want {sorted(want)}"
    print("self-test ok")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    found = check(tracked(), lambda p: open(p, "rb").read())
    if found:
        print("This repository is public; these must not be committed:", file=sys.stderr)
        for f in found:
            print("  " + f, file=sys.stderr)
        return 1
    print("public tree: no credentials, no agent notes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
