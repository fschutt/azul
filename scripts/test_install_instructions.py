#!/usr/bin/env python3
"""
test_install_instructions.py — verify every download URL referenced by the
per-language install instructions in api.json actually resolves (HTTP 200/302),
WITHOUT downloading bodies or compiling anything.

It walks the `installation.languages.<lang>` tree of api.json — both
`platforms.<os>.steps[]` and `methods.<m>.steps[]` — extracts the download
URLs out of every `curl` / `Invoke-WebRequest` step `content` string,
substitutes `$HOSTNAME`/`%HOSTNAME%` -> the hostname and `$VERSION`/`%VERSION%`
-> the version, and probes each distinct URL with a HEAD-style request.

Exit code is 0 when every *azul release* download URL (on `--hostname` or on
github.com/.../releases/download/) resolves, non-zero if any of them 404s (or
otherwise fails). Third-party download URLs referenced by an install step
(e.g. get.pharo.org) are still probed and reported, but a failure there does
NOT fail the run — they are not part of the azul release and would otherwise
make the deploy gate flaky on an unrelated outage. Use --strict to fail on any
broken URL regardless of host.

Pure stdlib (urllib + json + argparse). CI-friendly: a deploy job can run

    python3 scripts/test_install_instructions.py --hostname https://staging.example

to validate a staging deploy before promoting it.
"""

import argparse
import json
import os
import re
import shlex
import sys
import time
import urllib.error
import urllib.request
from collections import OrderedDict

# --------------------------------------------------------------------------
# api.json location (next to the repo root; this script lives in scripts/).
# --------------------------------------------------------------------------
_SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
_REPO_ROOT = os.path.dirname(_SCRIPT_DIR)
DEFAULT_API_JSON = os.path.join(_REPO_ROOT, "api.json")
DEFAULT_HOSTNAME = "https://azul.rs"

# curl flags that CONSUME the following token as an argument (so that token is
# not a URL). Long and short forms. Anything not in here that starts with '-'
# is treated as a valueless flag.
_CURL_FLAGS_WITH_ARG = {
    "-o", "--output",
    "-O",  # note: -O takes NO arg (uses remote name) — handled below, listed
           # here would be wrong, so it is intentionally NOT in this set.
    "--output-dir",
    "-w", "--write-out",
    "-H", "--header",
    "-d", "--data", "--data-raw", "--data-binary", "--data-urlencode",
    "-u", "--user",
    "-A", "--user-agent",
    "-e", "--referer",
    "-x", "--proxy",
    "-b", "--cookie",
    "-c", "--cookie-jar",
    "-T", "--upload-file",
    "-K", "--config",
    "-m", "--max-time",
    "--retry", "--retry-delay", "--retry-max-time",
    "--connect-timeout",
    "-X", "--request",
    "-r", "--range",
    "-Y", "--speed-limit",
    "-y", "--speed-time",
    "--cacert", "--capath", "--cert", "--key",
    "-E",
}
# -O explicitly takes no argument — remove it if it slipped in above.
_CURL_FLAGS_WITH_ARG.discard("-O")


def _looks_like_url(tok):
    """A token is a candidate download URL if it carries a scheme or one of the
    host placeholders we substitute."""
    if tok.startswith(("http://", "https://")):
        return True
    if "$HOSTNAME" in tok or "%HOSTNAME%" in tok:
        return True
    return False


def _urls_from_curl(tokens):
    """Given the argv tokens of a `curl` invocation (excluding the leading
    'curl'), return the URL operands, skipping flags and their arguments.

    Handles: `curl -O URL`, `curl -o local URL`, `curl -fsIL URL`,
    `curl -L URL | bash` (the '|' and beyond is dropped by the caller)."""
    urls = []
    i = 0
    n = len(tokens)
    while i < n:
        tok = tokens[i]
        if tok.startswith("-") and len(tok) > 1:
            # A flag. Does it consume the next token as an argument?
            # Long flags: exact match. Short clusters (e.g. -fsIL) never take
            # an argument here except when the WHOLE token is an arg-taking
            # short flag like -o / -w / -H (single letter after the dash).
            if tok in _CURL_FLAGS_WITH_ARG:
                i += 2  # skip the flag AND its argument
                continue
            # `-ofoo` style (flag glued to its arg) — short arg-taking flags.
            if len(tok) > 2 and tok[1] != "-" and ("-" + tok[1]) in _CURL_FLAGS_WITH_ARG:
                i += 1
                continue
            i += 1
            continue
        # Non-flag operand.
        if _looks_like_url(tok):
            urls.append(tok)
        i += 1
    return urls


def _urls_from_invoke_webrequest(tokens):
    """PowerShell `Invoke-WebRequest -Uri <URL> -OutFile <path>` (and the
    `iwr`/`curl`/`wget` aliases when used PowerShell-style). Returns URL
    operands. None exist in api.json today, but support keeps the extractor
    future-proof."""
    urls = []
    i = 0
    n = len(tokens)
    while i < n:
        tok = tokens[i]
        low = tok.lower()
        if low in ("-uri", "-u") and i + 1 < n:
            urls.append(tokens[i + 1])
            i += 2
            continue
        if low.startswith("-uri="):
            urls.append(tok.split("=", 1)[1])
            i += 1
            continue
        if low in ("-outfile", "-method", "-headers", "-body") and i + 1 < n:
            i += 2  # skip flag + its value
            continue
        if tok.startswith("-"):
            i += 1
            continue
        if _looks_like_url(tok):
            urls.append(tok)
        i += 1
    return urls


def extract_urls_from_content(content):
    """Pull every download URL out of one step `content` string.

    A step's content may have multiple lines and shell pipelines; we look at
    each pipeline segment, and if its leading command is a downloader
    (curl/wget/Invoke-WebRequest/iwr) we extract the URL operand(s)."""
    found = []
    for raw_line in content.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        # Split a pipeline / sequence into segments; only the leading token of
        # each segment decides whether it is a download command. We split on
        # the shell operators that start a new command.
        for segment in re.split(r"\|\||&&|\||;", line):
            seg = segment.strip()
            if not seg:
                continue
            try:
                tokens = shlex.split(seg, posix=True)
            except ValueError:
                # Unbalanced quotes etc. — fall back to a naive split so we at
                # least see obvious URLs.
                tokens = seg.split()
            if not tokens:
                continue
            cmd = os.path.basename(tokens[0]).lower()
            rest = tokens[1:]
            if cmd in ("curl", "wget"):
                found.extend(_urls_from_curl(rest))
            elif cmd in ("invoke-webrequest", "iwr"):
                found.extend(_urls_from_invoke_webrequest(rest))
            # Anything else (compile/run/package commands) is ignored, even if
            # it happens to contain a literal http URL in an argument we don't
            # recognise as a download.
    return found


def substitute(url, hostname, version):
    """Resolve $HOSTNAME/%HOSTNAME% and $VERSION/%VERSION% placeholders."""
    out = url
    out = out.replace("$HOSTNAME", hostname).replace("%HOSTNAME%", hostname)
    out = out.replace("${HOSTNAME}", hostname)
    out = out.replace("$VERSION", version).replace("%VERSION%", version)
    out = out.replace("${VERSION}", version)
    return out


def iter_steps(lang_entry):
    """Yield (context_label, step_dict) for every step under a language entry,
    across both `platforms.<os>` and `methods.<m>`."""
    for os_name, os_data in (lang_entry.get("platforms") or {}).items():
        if not isinstance(os_data, dict):
            continue
        for step in os_data.get("steps", []) or []:
            yield (os_name, step)
    for method_name, method_data in (lang_entry.get("methods") or {}).items():
        if not isinstance(method_data, dict):
            continue
        for step in method_data.get("steps", []) or []:
            yield (method_name, step)


def collect(api, hostname, version):
    """Return (rows, url_to_langs) where rows is an ordered list of
    (lang, context, resolved_url) and url_to_langs maps a resolved URL to the
    set of (lang, context) that reference it."""
    langs = api["installation"]["languages"]
    rows = []
    url_to_refs = OrderedDict()
    for lang in sorted(langs.keys()):
        entry = langs[lang]
        for context, step in iter_steps(entry):
            content = step.get("content", "") or ""
            for raw_url in extract_urls_from_content(content):
                resolved = substitute(raw_url, hostname, version)
                rows.append((lang, context, resolved))
                url_to_refs.setdefault(resolved, []).append((lang, context))
    return rows, url_to_refs


def classify_host(url, hostname):
    """'azul' for the release host or a github releases/download URL,
    else 'third-party'."""
    if url.startswith(hostname.rstrip("/") + "/"):
        return "azul"
    if "github.com/" in url and "/releases/download/" in url:
        return "azul"
    return "third-party"


def probe(url, retries, timeout):
    """HEAD-probe a URL without fetching the body. Returns (ok, code_or_err).

    ok is True for 200/302 (and other 2xx/3xx redirects that the opener
    follows to a final 200). Falls back to a tiny ranged GET if the server
    rejects HEAD (some static hosts 403/405 HEAD)."""
    last = None
    for attempt in range(retries + 1):
        # 1) Try HEAD.
        code = _request(url, method="HEAD", timeout=timeout)
        if isinstance(code, int) and (200 <= code < 400):
            return True, code
        # 2) Some hosts disallow HEAD — retry as a 1-byte ranged GET, which
        #    still avoids downloading the body.
        if code in (403, 405, 501) or not isinstance(code, int):
            code_get = _request(
                url, method="GET", timeout=timeout, headers={"Range": "bytes=0-0"}
            )
            if isinstance(code_get, int) and (200 <= code_get < 400):
                return True, code_get
            code = code_get if isinstance(code_get, int) else code
        last = code
        if isinstance(code, int) and 400 <= code < 500 and code != 429:
            # A definite client error (404 etc.) won't change on retry.
            return False, code
        if attempt < retries:
            time.sleep(min(2 ** attempt, 5))
    return False, last


def _request(url, method, timeout, headers=None):
    """Perform a single HTTP request following redirects. Returns the final
    status code (int) on completion, or an error string."""
    hdrs = {"User-Agent": "azul-install-tester/1.0"}
    if headers:
        hdrs.update(headers)
    req = urllib.request.Request(url, method=method, headers=hdrs)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.getcode()
    except urllib.error.HTTPError as e:
        return e.code
    except urllib.error.URLError as e:
        return "ERR:%s" % (getattr(e, "reason", e),)
    except Exception as e:  # noqa: BLE001 - report anything else as an error
        return "ERR:%s" % (e,)


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Verify install-instruction download URLs in api.json "
        "resolve (no download, no compile)."
    )
    ap.add_argument(
        "--api-json", default=DEFAULT_API_JSON,
        help="path to api.json (default: repo-root/api.json)",
    )
    ap.add_argument(
        "--hostname", default=None,
        help="override $HOSTNAME (default: %s)" % DEFAULT_HOSTNAME,
    )
    ap.add_argument(
        "--version", default=None,
        help="override $VERSION (default: the api.json version key)",
    )
    ap.add_argument(
        "--retry", type=int, default=2,
        help="retries per URL on transient failure (default: 2)",
    )
    ap.add_argument(
        "--timeout", type=float, default=30.0,
        help="per-request timeout in seconds (default: 30)",
    )
    ap.add_argument(
        "--strict", action="store_true",
        help="fail on ANY broken URL, including third-party hosts "
        "(default: only azul release URLs gate the exit code)",
    )
    ap.add_argument(
        "--list-only", action="store_true",
        help="print the resolved URLs and exit without probing",
    )
    args = ap.parse_args(argv)

    with open(args.api_json, "r", encoding="utf-8") as f:
        doc = json.load(f)

    # The api.json is keyed by version at the top level.
    version_key = sorted(doc.keys())[0] if len(doc) == 1 else max(doc.keys())
    api = doc[version_key]
    version = args.version or version_key
    hostname = (args.hostname or DEFAULT_HOSTNAME).rstrip("/")

    print("api.json:  %s" % args.api_json)
    print("version:   %s" % version)
    print("hostname:  %s" % hostname)
    print()

    rows, url_to_refs = collect(api, hostname, version)
    distinct = list(url_to_refs.keys())

    if args.list_only:
        for u in distinct:
            refs = ", ".join("%s/%s" % (l, c) for l, c in url_to_refs[u])
            print("%s\n    <- %s" % (u, refs))
        print("\n%d distinct URLs across %d references" % (len(distinct), len(rows)))
        return 0

    # Probe each distinct URL once; reuse results across the per-language table.
    results = {}
    for u in distinct:
        ok, code = probe(u, retries=args.retry, timeout=args.timeout)
        results[u] = (ok, code, classify_host(u, hostname))

    # ----- per-language / per-URL table -----
    print("STATUS  CODE   LANG / CONTEXT             URL")
    print("-" * 100)
    broken_azul = []
    broken_thirdparty = []
    ok_count = 0
    for lang, context, url in rows:
        ok, code, host = results[url]
        status = "PASS" if ok else "FAIL"
        tag = "" if host == "azul" else " [3rd-party]"
        print(
            "%-6s  %-5s  %-25s %s%s"
            % (status, str(code), "%s/%s" % (lang, context), url, tag)
        )
    print("-" * 100)

    # ----- summary over DISTINCT urls -----
    for u in distinct:
        ok, code, host = results[u]
        if ok:
            ok_count += 1
        elif host == "azul":
            broken_azul.append((u, code))
        else:
            broken_thirdparty.append((u, code))

    total = len(distinct)
    broken_total = len(broken_azul) + len(broken_thirdparty)
    print()
    print(
        "SUMMARY: %d urls, %d ok, %d broken (%d azul-release, %d third-party)"
        % (total, ok_count, broken_total, len(broken_azul), len(broken_thirdparty))
    )

    if broken_azul:
        print("\nBROKEN azul-release URLs (these FAIL the run):")
        for u, code in broken_azul:
            refs = ", ".join("%s/%s" % (l, c) for l, c in url_to_refs[u])
            print("  [%s] %s   <- %s" % (code, u, refs))
    if broken_thirdparty:
        note = "FAIL the run" if args.strict else "reported only, do not fail the run unless --strict"
        print("\nBROKEN third-party URLs (%s):" % note)
        for u, code in broken_thirdparty:
            refs = ", ".join("%s/%s" % (l, c) for l, c in url_to_refs[u])
            print("  [%s] %s   <- %s" % (code, u, refs))

    if broken_azul or (args.strict and broken_thirdparty):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
