#!/usr/bin/env python3
"""
scan_build_scripts.py — which dependencies may execute code at BUILD time.

THE THREAT. `build.rs` is the kill chain in every significant crates.io supply
chain attack to date. It runs during `cargo build` with the full authority of
whoever typed the command — filesystem, network, environment — and it runs
whether or not any of the crate's actual code is ever called. Proc macros are
the same authority with less visibility: they run inside rustc. The August 2026
`arrayref` / `internment` / `append-only-vec` compromise did not put a single
byte of payload in those crates; it added one line to Cargo.toml pulling in
`proc-macro1` (a typosquat of proc-macro2), whose build.rs downloaded and ran a
remote payload. Nobody had to call anything.

WHAT THIS GATE DOES. It reads a `cargo vendor` tree, finds every crate that
executes code at build time, and requires each one to have an entry in
`build-script-policy.toml` that carries:

    allow     — may this crate run code at build time at all
    risk      — the risk level a human ACKNOWLEDGED when they read it
    reason    — why this crate needs a build script, in one line
    reviewed  — ["<version>:<sha256>", ...] the exact bytes that were read

`reviewed` is the part that `dependency-justifications.toml` cannot do. That
file answers "why is this crate in the tree", keyed by NAME — so a crate that is
already justified stays justified when a compromised account publishes a new
version of it. The digest here is over the build script's actual content, so a
new version, a new payload, or a payload moved sideways into `build/probe.rs`
all read as a mismatch and fail the build until a human looks at the diff.

It also classifies each build script against a table of behaviours that
distinguish a legitimate build script from an exfiltration stub (see RULES
below), so review has somewhere to start and a NEW dangerous behaviour in an
already-pinned crate is called out by name.

Usage:
    scan_build_scripts.py --vendor vendor/ [--policy FILE] [--report-only]
    scan_build_scripts.py --vendor vendor/ --update      # refresh digests/skeletons
    scan_build_scripts.py --print-rules                  # the rule table

Exit 0 when every build-time crate is allowed, pinned and unchanged; exit 1
naming the offenders otherwise.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import sc_common as sc  # noqa: E402

DEFAULT_POLICY = Path(__file__).resolve().parent / "build-script-policy.toml"

# ---------------------------------------------------------------------------
# RULES — behaviours that separate a real build script from a payload.
#
# Each rule is (id, weight, description, compiled regex). Weights add up into a
# score; the score picks a level. The weights are deliberately lopsided: a build
# script spawning a C compiler is completely ordinary (`cc`, `bindgen` and every
# -sys crate do it), while a build script opening a socket has, essentially, no
# legitimate reason to exist — cargo has already downloaded everything the crate
# is allowed to have by the time build.rs runs.
#
# False positives are expected and fine. This table does not decide anything on
# its own; it decides where a human looks first, and it turns "this crate's
# build script started doing something new" into a build failure rather than a
# thing nobody noticed.
# ---------------------------------------------------------------------------

RULES: list[tuple[str, int, str, re.Pattern]] = [
    # ---- network: the exfiltration/download primitive itself -------------
    ("net.std", 45, "std::net socket use in a build script", re.compile(
        r'\b(?:std::)?net::(?:TcpStream|TcpListener|UdpSocket|ToSocketAddrs)\b'
        r'|\bTcpStream::connect\b|\bUdpSocket::bind\b|\bto_socket_addrs\s*\(')),
    ("net.http_client", 50, "HTTP client crate used at build time", re.compile(
        r'\b(?:reqwest|ureq|isahc|attohttpc|minreq|hyper|surf|curl|awc)\s*::')),
    ("net.download_tool", 55, "build script shells out to a downloader", re.compile(
        r'Command::new\s*\(\s*"(?:curl|wget|Invoke-WebRequest|powershell|pwsh|nc|ncat)"')),
    ("net.url_literal", 15, "http(s) URL literal in build-time code", re.compile(
        r'"https?://[^"\s]{6,}"')),
    # `resolv` + \w* used to be in this alternation. It matched `resolve` and
    # `resolved_at` — so every proc-macro crate in the tree that resolves a
    # path or a span scored 35 for "DNS", and thiserror-impl and clap_derive
    # came out HIGH on a rule neither of them trips. Match real APIs only.
    ("net.dns", 35, "DNS resolution / tunneling primitive", re.compile(
        r'\b(?:trust_dns|hickory_dns)\w*\s*::|\bres_query\s*\(|\bDnsQuery\w*\s*\('
        r'|\bgetaddrinfo\s*\(|\bresolv\.conf\b')),

    # ---- environment: the credential-theft primitive ---------------------
    ("env.enumerate", 55, "enumerates the ENTIRE environment (vars/vars_os)", re.compile(
        r'\b(?:std::)?env::vars(?:_os)?\s*\(')),
    ("env.ci_detect", 45, "branches on a CI-provider variable (rustdecimal tell)", re.compile(
        r'"(?:GITLAB_CI|GITHUB_ACTIONS|GITHUB_TOKEN|TRAVIS|CIRCLECI|JENKINS_URL'
        r'|BUILDKITE|TF_BUILD|TEAMCITY_VERSION|APPVEYOR|DRONE|CODEBUILD_BUILD_ID)"')),
    ("env.secretish", 60, "reads a token/secret/credential-shaped variable", re.compile(
        r'(?:env!|option_env!|env::var(?:_os)?)\s*\(\s*"[^"]*'
        r'(?:TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIAL|API_KEY|APIKEY|PRIVATE_KEY)[^"]*"',
        re.IGNORECASE)),

    # ---- process execution ----------------------------------------------
    ("proc.shell", 50, "build script spawns a SHELL", re.compile(
        r'Command::new\s*\(\s*"(?:sh|bash|zsh|dash|cmd|cmd\.exe)"'
        r'|\.args?\s*\(\s*&?\[\s*"-c"')),
    ("proc.spawn", 8, "spawns a subprocess (normal for -sys/cc/bindgen crates)", re.compile(
        r'\bCommand::new\s*\(')),

    # ---- filesystem outside the sandbox ----------------------------------
    ("fs.credentials", 70, "touches a credential store path", re.compile(
        r'"[^"]*(?:\.ssh|\.cargo/credentials|\.aws/credentials|\.npmrc|\.netrc'
        r'|id_rsa|id_ed25519|\.gnupg|Keychains)[^"]*"')),
    ("fs.home", 30, "resolves the user's HOME/profile directory", re.compile(
        r'\b(?:home_dir|home::home_dir|dirs::(?:home|config|data)_dir)\s*\('
        r'|env::var(?:_os)?\s*\(\s*"(?:HOME|USERPROFILE|APPDATA)"')),
    ("fs.write_abs", 20, "writes to an absolute/system path", re.compile(
        r'(?:File::create|fs::write|OpenOptions::new)[^\n]{0,80}"(?:/etc/|/usr/|/bin/'
        r'|/var/|C:\\\\Windows|/Library/LaunchAgents)')),

    # ---- code loading / obfuscation --------------------------------------
    ("load.dynamic", 35, "loads a shared object at build time", re.compile(
        r'\blibloading\s*::|\bdlopen\s*\(|\bLoadLibrary[AW]?\s*\(|\bdlsym\s*\(')),
    ("obf.long_b64", 45, "long base64-shaped literal (packed payload)", re.compile(
        r'"[A-Za-z0-9+/]{220,}={0,2}"')),
    ("obf.hex_escape", 40, "long run of \\x byte escapes", re.compile(
        r'(?:\\x[0-9a-fA-F]{2}){48,}')),
    ("obf.decode", 30, "decodes an embedded blob at build time", re.compile(
        r'\b(?:base64|hex)\s*::\s*decode\b|\bfrom_base64\b|\bBASE64[A-Z_]*\.decode\b')),
    ("obf.xor", 35, "XOR/reverse deobfuscation loop", re.compile(
        r'\.iter\(\)[^\n]{0,60}\|\s*\w+\s*\|\s*\w+\s*\^|\bchars\(\)\s*\.\s*rev\s*\(\)')),
    ("obf.exec_written", 60, "makes a file executable at build time", re.compile(
        r'set_permissions|PermissionsExt|from_mode\s*\(\s*0o7|chmod')),
]

LEVELS = ("low", "medium", "high")
THRESHOLD_HIGH = 45
THRESHOLD_MEDIUM = 15

# Rules that describe *ordinary* build-script work. They still count toward the
# score (a shell-out is how you'd invoke a downloader), but they must never be
# the sole reason a crate is called out — otherwise every -sys crate is "high"
# and the signal is gone.
BENIGN_ONLY = {"proc.spawn", "net.url_literal", "fs.home"}

# Rules that still mean something when applied to a whole LIBRARY rather than a
# build script. The rest of the table assumes it is reading a two-hundred-line
# build.rs where every line is purposeful; run against fifty thousand lines of
# FFI declarations it produces noise and nothing else. Measured, before this
# split: `windows-sys` scored "high" on net.dns because it DECLARES DnsQuery and
# on load.dynamic because it declares LoadLibraryA; `linux-raw-sys` scored 1200
# on obf.exec_written because it is a crate of syscall constants;
# `webpki-root-certs` tripped obf.hex_escape because it is, by design, a file of
# hex-encoded root certificates. Fifteen "high" findings, none of them real.
#
# What survives the move to library scope is the exfiltration set — reading the
# environment, detecting CI, shelling out to a downloader — because those are
# behaviours, not vocabulary, and a library has no more business doing them than
# a build script does. `built` 0.7.7 is still caught here.
LIBRARY_RULES = {
    "env.enumerate", "env.ci_detect", "env.secretish",
    "net.download_tool", "net.http_client", "proc.shell", "obf.long_b64",
}


def classify(paths: list[Path], scope: str = "build-script") -> tuple[str, int, list[dict]]:
    """Score a set of build-time files. Returns (level, score, hits).

    `scope` is "build-script" for a real build script or proc macro, and
    "library" for a build-dependency whose ordinary library code happens to run
    at build time; see LIBRARY_RULES.
    """
    active = RULES if scope != "library" else [r for r in RULES if r[0] in LIBRARY_RULES]
    hits: list[dict] = []
    score = 0
    for path in paths:
        if path.suffix not in (".rs", ".c", ".h", ".cc", ".cpp", ".py", ".sh"):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        # Comments are not behaviour, and neither are `#[test]` items — rustc
        # only compiles those under `--test`. Stripping both removes the two
        # biggest sources of noise here: every other -sys crate documents a URL
        # in a comment, and test helpers routinely read credential-shaped
        # variables that no build ever touches.
        stripped = sc.normalise_source(text)
        for rule_id, weight, desc, pat in active:
            found = pat.findall(stripped)
            if not found:
                continue
            score += weight
            sample = found[0] if isinstance(found[0], str) else str(found[0])
            hits.append({
                "rule": rule_id, "weight": weight, "desc": desc,
                "file": path.name, "count": len(found), "sample": sample[:120],
            })
    substantive = [h for h in hits if h["rule"] not in BENIGN_ONLY]
    if not substantive:
        return "low", score, hits
    if score >= THRESHOLD_HIGH:
        return "high", score, hits
    if score >= THRESHOLD_MEDIUM:
        return "medium", score, hits
    return "low", score, hits


def rank(level: str) -> int:
    return LEVELS.index(level)


# ---------------------------------------------------------------------------

def collect(vendor: Path, build_deps: set[str] | None = None) -> list[dict]:
    """Every crate that runs code at build time, with digest and risk.

    `build_deps` is the transitive `[build-dependencies]` closure. Those crates
    have no build script and are not proc macros, but their library code is
    linked into build scripts and therefore executes with the same authority —
    `built` 0.7.7 reaches azul that way and sweeps the whole environment. They
    are scanned, but (see `evaluate`) only need a policy entry if they actually
    exhibit non-trivial behaviour; requiring a hand-written line for all 178 of
    them would bury the ~170 that matter under boilerplate.
    """
    build_deps = build_deps or set()
    out = []
    for crate in sc.walk_vendor(vendor):
        facts = sc.manifest_facts(crate)
        bpaths = sc.build_script_paths(crate)
        is_pm = facts["proc_macro"]
        is_bd = crate.name in build_deps
        if not bpaths and not is_pm and not is_bd:
            continue
        # A proc-macro crate's *sources* are build-time code, but hashing all of
        # src/ would repin on every cosmetic release. Digest the build script
        # when there is one; for a pure proc-macro crate, digest src/ — its
        # sources ARE the executed code and there is nothing else to pin.
        paths = bpaths if bpaths else sc.proc_macro_paths(crate)
        scope = "library" if (not bpaths and not is_pm) else "build-script"
        level, score, hits = classify(paths, scope)
        out.append({
            "name": crate.name,
            "version": crate.version,
            "kind": ("build-script+proc-macro" if bpaths and is_pm
                     else "build-script" if bpaths
                     else "proc-macro" if is_pm else "build-dependency"),
            "escapes": sc.build_script_escapes(crate),
            "links": facts["links"],
            "files": [str(p.relative_to(crate.path)).replace("\\", "/") for p in paths],
            "digest": sc.digest_files(crate.path, paths),
            "risk": level,
            "score": score,
            "hits": hits,
        })
    return sorted(out, key=lambda d: (d["name"], d["version"]))


def load_policy(path: Path) -> dict:
    doc = sc.read_toml_subset(path)
    return doc.get("crate", {})


def evaluate(found: list[dict], policy: dict) -> tuple[list[dict], list[dict]]:
    """Returns (violations, ok_entries)."""
    violations, ok = [], []
    for entry in found:
        rule = policy.get(entry["name"])
        pin = f'{entry["version"]}:{entry["digest"]}'
        # A reference escaping the crate directory is always a violation: a
        # vendored crate has no legitimate reason to compile a file it does not
        # ship, and it is how a pin over "this crate's files" gets bypassed.
        if entry.get("escapes"):
            violations.append({**entry, "why": "ESCAPES", "detail":
                               "build-time code references files outside the crate: "
                               + ", ".join(entry["escapes"][:3])})
            continue
        if rule is None:
            violations.append({**entry, "why": "UNREVIEWED", "detail":
                               "no entry in build-script-policy.toml"})
            continue
        if not rule.get("allow", False):
            violations.append({**entry, "why": "DENIED", "detail":
                               rule.get("reason", "allow = false")})
            continue
        reviewed = rule.get("reviewed", [])
        if pin not in reviewed:
            known = [r.split(":", 1)[0] for r in reviewed]
            if entry["version"] in known:
                detail = (f'build-time code CHANGED for {entry["name"]} '
                          f'{entry["version"]} without a version bump — '
                          f'expected {[r for r in reviewed if r.startswith(entry["version"] + ":")][0][:24]}…, '
                          f'got {entry["digest"][:16]}…')
            else:
                detail = (f'version {entry["version"]} is not reviewed '
                          f'(reviewed: {", ".join(known) or "none"})')
            violations.append({**entry, "why": "DIGEST", "detail": detail})
            continue
        # A build script or proc macro is code that asked to run. It does not
        # get to hide behind the generated descriptor that build-dependency
        # libraries carry — those were pinned, not read, and the file says so.
        if entry["kind"] != "build-dependency" and rule.get("review") == "digest-only":
            violations.append({**entry, "why": "UNAUDITED", "detail":
                               "has its own build-time entry point but is marked "
                               "review = \"digest-only\"; it needs a written reason"})
            continue
        ack = rule.get("risk", "low")
        if rank(entry["risk"]) > rank(ack):
            violations.append({**entry, "why": "RISK", "detail":
                               f'behaviour scores {entry["risk"]} but policy acknowledges '
                               f'{ack}: ' + "; ".join(sorted({h["desc"] for h in entry["hits"]
                                                              if h["rule"] not in BENIGN_ONLY}))})
            continue
        ok.append({**entry, "reason": rule.get("reason", "")})
    return violations, ok


_GENERATED_LIB_REASON = (
    "build-dependency: ordinary library linked into other crates' build "
    "scripts; no build-time entry point of its own")


def render_policy(found: list[dict], policy: dict) -> str:
    """Regenerate the policy file, preserving human-written reasons."""
    lines = [
        "# build-script-policy.toml — which dependencies may execute code at BUILD time.",
        "#",
        "# GENERATED SKELETON, HUMAN-OWNED CONTENT. `scan_build_scripts.py --update`",
        "# refreshes `reviewed` digests and adds skeletons for new crates; it never",
        "# invents a `reason` and never flips an `allow`. A crate whose reason is still",
        "# the TODO placeholder has not been reviewed by anybody.",
        "#",
        "# Fields:",
        "#   allow    — may this crate run code at build time at all",
        "#   risk     — behaviour level a human ACKNOWLEDGED: low | medium | high",
        "#   review   — audited | digest-only (see below)",
        "#   reason   — one line: why does this crate need to run code at build time",
        "#   reviewed — [\"<version>:<sha256-of-build-time-files>\"] — the exact bytes pinned",
        "#",
        "# TWO TIERS, and this file does not pretend otherwise.",
        "#",
        "#   review = \"audited\"      somebody read this crate's build-time code and",
        "#                          wrote the reason. Every crate with its own",
        "#                          build.rs or proc macro is here, plus the",
        "#                          build dependencies that behave interestingly",
        "#                          enough to have been read (built, git2, ureq…).",
        "#",
        "#   review = \"digest-only\"  an ordinary library — regex, chrono, cc — that",
        "#                          never asked to run at build time and is only",
        "#                          here because someone else's build script links",
        "#                          it. Nobody read it. Its BYTES ARE PINNED, which",
        "#                          is what detects a change; the sentence beside",
        "#                          it is generated and carries no claim.",
        "#",
        "# A crate with its own build-time entry point may not be digest-only: the",
        "# gate rejects that combination. Downgrading an audited entry to",
        "# digest-only is therefore a deliberate act, not something --update does.",
        "#",
        "# Re-pinning after a version bump is DELIBERATELY manual: the digest changing",
        "# is the signal that somebody has to read the diff. `--update` writes the new",
        "# digest so the mechanics are cheap, but the review is the point.",
        "",
    ]
    by_name: dict[str, list[dict]] = {}
    for e in found:
        by_name.setdefault(e["name"], []).append(e)
    for name in sorted(by_name):
        entries = by_name[name]
        old = policy.get(name, {})
        kinds = "/".join(sorted({e["kind"] for e in entries}))
        # TWO TIERS, and the file says which is which.
        #
        # A crate with its own build script or proc macro ASKED to run at build
        # time; somebody has to write down why, and `review = "audited"` records
        # that they did. A build-DEPENDENCY is an ordinary library that never
        # asked — `regex`, `chrono`, `cc` — pulled in because someone else's
        # build script links it. Demanding hand-written prose for 163 of those
        # produces 163 lines of nothing and buries the ones that matter.
        #
        # They are still PINNED, which is the point: the digest is what detects
        # a change, not the sentence next to it. `review = "digest-only"` is an
        # honest label for "nobody read this crate; we pinned its bytes".
        is_lib = all(e["kind"] == "build-dependency" for e in entries)
        had_written_reason = bool(old.get("reason")) and not str(
            old["reason"]).startswith(("TODO", _GENERATED_LIB_REASON[:40]))
        # A build-DEPENDENCY that somebody actually read keeps `audited`. The
        # first cut derived the tier from `kind` alone, which relabelled built,
        # git2, ureq, libloading and a dozen others as "digest-only" — crates a
        # reviewer had read line by line. A security record that understates
        # what was reviewed is as bad as one that overstates it.
        review = old.get("review") or (
            "audited" if had_written_reason or not is_lib else "digest-only")
        if old.get("reason"):
            reason = old["reason"]
        elif is_lib:
            reason = _GENERATED_LIB_REASON
        else:
            reason = "TODO: why does this crate run code at build time?"
        ack = old.get("risk") or max((e["risk"] for e in entries), key=rank)
        allow = old.get("allow", True)
        pins = sorted(f'{e["version"]}:{e["digest"]}' for e in entries)
        lines.append(f'[crate."{name}"]  # {kinds}')
        lines.append(f'allow = {"true" if allow else "false"}')
        lines.append(f'risk = "{ack}"')
        lines.append(f'review = "{review}"')
        lines.append(f'reason = "{reason.replace(chr(92), chr(92)*2).replace(chr(34), chr(92) + chr(34))}"')
        lines.append("reviewed = [" + ", ".join(f'"{p}"' for p in pins) + "]")
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--vendor", type=Path, default=Path("vendor"))
    ap.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    ap.add_argument("--json", type=Path, help="write the full finding set here")
    ap.add_argument("--update", action="store_true",
                    help="rewrite the policy file: refresh digests, add skeletons")
    ap.add_argument("--report-only", action="store_true", help="never exit non-zero")
    ap.add_argument("--print-rules", action="store_true")
    ap.add_argument("--metadata", type=Path,
                    help="cargo metadata JSON; without it the build-dependency "
                         "closure is computed by invoking cargo")
    ap.add_argument("--no-build-deps", action="store_true",
                    help="scan only build scripts and proc macros")
    args = ap.parse_args()

    if args.print_rules:
        print(f"{'rule':22s} {'wt':>3s}  description")
        for rid, w, desc, _ in RULES:
            print(f"{rid:22s} {w:3d}  {desc}"
                  + ("   [benign-only]" if rid in BENIGN_ONLY else ""))
        print(f"\nhigh >= {THRESHOLD_HIGH}, medium >= {THRESHOLD_MEDIUM}, "
              f"and at least one non-benign hit is required for either.")
        return 0

    build_deps: set[str] = set()
    if not args.no_build_deps:
        build_deps = sc.build_dependency_closure(sc.load_metadata(args.metadata))
    found = collect(args.vendor, build_deps)
    policy = load_policy(args.policy) if args.policy.is_file() else {}

    if args.update:
        args.policy.write_text(render_policy(found, policy), encoding="utf-8")
        todo = sum(1 for n, r in load_policy(args.policy).items()
                   if str(r.get("reason", "")).startswith("TODO"))
        print(f"wrote {args.policy} — {len(found)} build-time crates, {todo} awaiting a written reason")
        return 0

    violations, ok = evaluate(found, policy)
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(
            {"found": found, "violations": violations}, indent=2), encoding="utf-8")

    # ---- report ----
    by_risk = {lvl: [e for e in found if e["risk"] == lvl] for lvl in LEVELS}
    md = ["## 🔧 Build-time code execution (build.rs / proc-macro)", "",
          f"`{len(found)}` of `{len(sc.walk_vendor(args.vendor))}` vendored crates run code at build time — "
          f"**{len(by_risk['high'])} high**, {len(by_risk['medium'])} medium, {len(by_risk['low'])} low risk.", ""]
    if violations:
        md += ["### ❌ Violations", "",
               "| crate | version | why | detail |", "|---|---|---|---|"]
        for v in violations:
            md.append(f'| `{v["name"]}` | {v["version"]} | **{v["why"]}** | {v["detail"]} |')
        md.append("")
    for lvl in ("high", "medium"):
        if not by_risk[lvl]:
            continue
        md += [f"<details><summary><b>{lvl} risk ({len(by_risk[lvl])})</b></summary>", "",
               "| crate | score | behaviours |", "|---|---|---|"]
        for e in by_risk[lvl]:
            beh = ", ".join(sorted({h["desc"] for h in e["hits"] if h["rule"] not in BENIGN_ONLY}))
            md.append(f'| `{e["name"]} {e["version"]}` | {e["score"]} | {beh} |')
        md += ["", "</details>", ""]
    sc.summary("\n".join(md))

    print(f"build-time crates: {len(found)}  "
          f"(high={len(by_risk['high'])} medium={len(by_risk['medium'])} low={len(by_risk['low'])})")
    if not violations:
        print(f"all {len(ok)} allowed, pinned by digest, and unchanged since review")
        return 0
    for v in violations:
        sc.annotate("error", f'{v["name"]} {v["version"]}: {v["why"]} — {v["detail"]}')
    print(f"\n{len(violations)} violation(s). Review the build script, then re-pin with:\n"
          f"    python3 {sc.rel_to_repo(Path(__file__))} --vendor {args.vendor} --update")
    return 0 if args.report_only else 1


if __name__ == "__main__":
    sys.exit(main())
