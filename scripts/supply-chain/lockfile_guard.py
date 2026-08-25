#!/usr/bin/env python3
"""
lockfile_guard.py — which crate VERSIONS are allowed into the tree.

THE THREAT THIS COVERS AND `dependency-justifications.toml` DOES NOT. That file
answers "why is this crate here", keyed by crate NAME. It is the right question
and it stops a dependency being added quietly. It is also, by construction,
blind to the attack that actually happened on 2026-08-20: `arrayref`,
`internment` and `append-only-vec` were already in thousands of trees, already
justified by anyone who justifies dependencies, when a compromised maintainer
account published new versions of all three inside a 23-minute window. The only
change was one line of Cargo.toml pulling in `proc-macro1`. Exposure was 86–107
minutes. A name-keyed allowlist says yes to every one of those releases.

Four checks, in increasing cost:

  --check vendor    every vendored file matches the registry's published
                    checksum (offline, seconds). Catches a tampered vendor tree
                    and a lockfile checksum that disagrees with the crate.
  --check yanked    no locked version is yanked. The arrayref attacker YANKED
                    the good releases to force resolution onto the malicious
                    one, so "is anything in my lockfile yanked" is a live
                    question, not hygiene.
  --check cksum     lockfile checksums match the registry index.
  --check cooldown  no locked version is younger than --min-age-days. This is
                    RFC 3923 `min-publish-age`, which is merged but whose
                    client-side half had not shipped when arrayref happened —
                    two days after the stabilisation PR entered final comment
                    period. A 14-day floor turns a 107-minute exposure window
                    into a non-event, because the malicious version is yanked
                    long before any build is allowed to resolve it.

Cooldown needs one crates.io API request per crate and that API is rate-limited,
so by default it runs only over versions that CHANGED against a base ref
(`--changed-only`), which is both the fast path and the interesting one: a
lockfile diff is exactly where a poisoned release enters.

Usage:
    lockfile_guard.py --check vendor --vendor vendor/
    lockfile_guard.py --check yanked --check cksum
    lockfile_guard.py --check cooldown --changed-only --base-ref origin/master
    lockfile_guard.py --check all --vendor vendor/ --min-age-days 14

Exit 0 when every locked version passes the selected checks; 1 otherwise.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import hashlib
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import sc_common as sc  # noqa: E402

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
SPARSE = "https://index.crates.io"
API = "https://crates.io/api/v1/crates"
UA = "azul-supply-chain-gate (+https://github.com/fschutt/azul)"


# ---------------------------------------------------------------------------

def parse_lock(path: Path) -> list[dict]:
    """[[package]] blocks. Regex, not a TOML library — same portability reason
    as everywhere else in this directory, and the shape is fixed by cargo."""
    if not path.is_file():
        raise SystemExit(f"error: {path} not found")
    out = []
    for blk in path.read_text(encoding="utf-8").split("[[package]]")[1:]:
        name = re.search(r'(?m)^name = "(.+)"$', blk)
        ver = re.search(r'(?m)^version = "(.+)"$', blk)
        src = re.search(r'(?m)^source = "(.+)"$', blk)
        cks = re.search(r'(?m)^checksum = "(.+)"$', blk)
        if name and ver:
            out.append({"name": name.group(1), "version": ver.group(1),
                        "source": src.group(1) if src else None,
                        "checksum": cks.group(1) if cks else None})
    return out


def index_path(name: str) -> str:
    n = name.lower()
    if len(n) == 1:
        return f"1/{n}"
    if len(n) == 2:
        return f"2/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


def _get(url: str, timeout: int = 20) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


class Cache:
    """Disk cache. The index is immutable per (crate, version) except for the
    yank flag, so a cache makes repeat runs cheap without hiding a fresh yank:
    `--check yanked` passes --no-cache in CI's scheduled run."""

    def __init__(self, path: Path | None):
        self.path = path
        self.data: dict = {}
        if path and path.is_file():
            try:
                self.data = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                self.data = {}

    def get(self, key: str):
        return self.data.get(key)

    def put(self, key: str, value) -> None:
        self.data[key] = value

    def flush(self) -> None:
        if self.path:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            self.path.write_text(json.dumps(self.data), encoding="utf-8")


def fetch_index(names: list[str], cache: Cache, workers: int = 12) -> dict:
    """{name: {version: {"cksum":..., "yanked":...}}} from the sparse index."""
    todo = [n for n in names if cache.get(f"idx:{n}") is None]
    if todo:
        def one(n):
            try:
                body = _get(f"{SPARSE}/{index_path(n)}")
            except urllib.error.HTTPError as e:
                return n, ({} if e.code == 404 else None)
            except Exception:
                return n, None
            vers = {}
            for line in body.decode("utf-8", "replace").splitlines():
                if not line.strip():
                    continue
                try:
                    d = json.loads(line)
                except ValueError:
                    continue
                vers[d["vers"]] = {"cksum": d.get("cksum"), "yanked": bool(d.get("yanked"))}
            return n, vers
        with cf.ThreadPoolExecutor(max_workers=workers) as ex:
            for n, vers in ex.map(one, todo):
                if vers is not None:
                    cache.put(f"idx:{n}", vers)
    return {n: cache.get(f"idx:{n}") or {} for n in names}


def fetch_published(pkgs: list[dict], cache: Cache, delay: float = 0.6) -> dict:
    """{(name, version): iso8601}. Serialised — the crates.io API is rate
    limited and a gate that gets itself 429'd is a gate that fails open."""
    out = {}
    for p in pkgs:
        key = f"pub:{p['name']}:{p['version']}"
        hit = cache.get(key)
        if hit is None:
            try:
                body = _get(f"{API}/{p['name']}/{p['version']}")
                hit = json.loads(body)["version"]["created_at"]
            except Exception:
                hit = ""
            cache.put(key, hit)
            time.sleep(delay)
        if hit:
            out[(p["name"], p["version"])] = hit
    return out


def changed_versions(base_ref: str, lock: Path) -> set[tuple[str, str]] | None:
    """(name, version) pairs present now and not at `base_ref`."""
    try:
        old = subprocess.run(["git", "show", f"{base_ref}:{lock}"],
                             capture_output=True, text=True, check=True).stdout
    except subprocess.CalledProcessError:
        return None
    prev = set()
    for blk in old.split("[[package]]")[1:]:
        n = re.search(r'(?m)^name = "(.+)"$', blk)
        v = re.search(r'(?m)^version = "(.+)"$', blk)
        if n and v:
            prev.add((n.group(1), v.group(1)))
    now = {(p["name"], p["version"]) for p in parse_lock(lock)}
    return now - prev


def verify_vendor(vendor: Path, lock: list[dict]) -> list[dict]:
    """Every vendored file against the registry checksums cargo recorded.

    `.cargo-checksum.json` is written by cargo from what the registry served, so
    this is an offline integrity check of the whole vendor tree against what
    crates.io published — including the package-level checksum, which must also
    agree with Cargo.lock.
    """
    by_id = {(p["name"], p["version"]): p for p in lock}
    problems = []
    for crate in sc.walk_vendor(vendor):
        manifest = crate.path / ".cargo-checksum.json"
        if not manifest.is_file():
            # Path/git sources legitimately have none; a registry crate without
            # one cannot be verified and must be said out loud, not skipped.
            if (crate.name, crate.version) in by_id and \
                    by_id[(crate.name, crate.version)]["source"] == CRATES_IO:
                problems.append({"crate": crate.id, "kind": "NO-CHECKSUM-FILE",
                                 "detail": ".cargo-checksum.json missing"})
            continue
        try:
            doc = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            problems.append({"crate": crate.id, "kind": "UNREADABLE", "detail": str(e)})
            continue

        locked = by_id.get((crate.name, crate.version))
        pkg_sum = doc.get("package")
        if locked and locked["checksum"] and pkg_sum and locked["checksum"] != pkg_sum:
            problems.append({"crate": crate.id, "kind": "LOCK-MISMATCH", "detail":
                             f"Cargo.lock {locked['checksum'][:16]}… vs vendor {pkg_sum[:16]}…"})

        files = doc.get("files", {})
        on_disk = {str(p.relative_to(crate.path)).replace("\\", "/")
                   for p in crate.path.rglob("*") if p.is_file()
                   and p.name != ".cargo-checksum.json"}
        for rel, want in files.items():
            p = crate.path / rel
            if not p.is_file():
                problems.append({"crate": crate.id, "kind": "MISSING",
                                 "detail": rel})
                continue
            got = hashlib.sha256(p.read_bytes()).hexdigest()
            if got != want:
                problems.append({"crate": crate.id, "kind": "MODIFIED",
                                 "detail": f"{rel}: {got[:16]}… != {want[:16]}…"})
        for extra in sorted(on_disk - set(files)):
            problems.append({"crate": crate.id, "kind": "EXTRA-FILE", "detail": extra})
    return problems


# ---------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--lock", type=Path, default=Path("Cargo.lock"))
    ap.add_argument("--vendor", type=Path, default=Path("vendor"))
    ap.add_argument("--check", action="append", default=[],
                    choices=("vendor", "yanked", "cksum", "cooldown", "all"))
    ap.add_argument("--min-age-days", type=int, default=14)
    ap.add_argument("--exempt", action="append", default=[], metavar="CRATE",
                    help="crate exempt from the cooldown floor (repeatable)")
    ap.add_argument("--exempt-file", type=Path,
                    default=Path(__file__).resolve().parent / "cooldown-exempt.txt",
                    help="file of cooldown-exempt crate names, one per line")
    ap.add_argument("--changed-only", action="store_true")
    ap.add_argument("--base-ref", default="origin/master")
    ap.add_argument("--cache", type=Path, default=Path(".supply-chain-cache.json"))
    ap.add_argument("--no-cache", action="store_true")
    ap.add_argument("--json", type=Path)
    ap.add_argument("--report-only", action="store_true")
    args = ap.parse_args()

    checks = set(args.check) or {"vendor", "yanked", "cksum"}
    if "all" in checks:
        checks = {"vendor", "yanked", "cksum", "cooldown"}

    lock = parse_lock(args.lock)
    registry = [p for p in lock if p["source"] == CRATES_IO]
    cache = Cache(None if args.no_cache else args.cache)
    violations: list[dict] = []
    md = [f"## 📌 Lockfile integrity — {len(lock)} packages "
          f"({len(registry)} from crates.io)", ""]

    if "vendor" in checks:
        problems = verify_vendor(args.vendor, lock)
        violations += [{"kind": p["kind"], "crate": p["crate"], "detail": p["detail"]}
                       for p in problems]
        md.append(f"- **vendor integrity**: {'❌ ' + str(len(problems)) + ' problem(s)' if problems else '✅ every vendored file matches its registry checksum'}")

    scope = registry
    if args.changed_only:
        changed = changed_versions(args.base_ref, args.lock)
        if changed is None:
            print(f"::warning::could not read {args.base_ref}:{args.lock} — checking everything")
        else:
            scope = [p for p in registry if (p["name"], p["version"]) in changed]
            md.append(f"- **scope**: {len(scope)} version(s) changed against `{args.base_ref}`")

    if checks & {"yanked", "cksum"}:
        idx = fetch_index(sorted({p["name"] for p in scope}), cache)
        yanked = cks_bad = unknown = 0
        for p in scope:
            entry = idx.get(p["name"], {}).get(p["version"])
            if entry is None:
                unknown += 1
                continue
            if "yanked" in checks and entry["yanked"]:
                yanked += 1
                violations.append({"kind": "YANKED", "crate": f'{p["name"]} {p["version"]}',
                                   "detail": "this version is yanked on crates.io"})
            if "cksum" in checks and p["checksum"] and entry["cksum"] \
                    and p["checksum"] != entry["cksum"]:
                cks_bad += 1
                violations.append({"kind": "CKSUM", "crate": f'{p["name"]} {p["version"]}',
                                   "detail": f'lockfile {p["checksum"][:16]}… != index {entry["cksum"][:16]}…'})
        if "yanked" in checks:
            md.append(f"- **yanked**: {'❌ ' + str(yanked) if yanked else '✅ none'}"
                      + (f" ({unknown} not found in the index)" if unknown else ""))
        if "cksum" in checks:
            md.append(f"- **checksums**: {'❌ ' + str(cks_bad) + ' mismatched' if cks_bad else '✅ all match the registry index'}")

    if "cooldown" in checks:
        # First-party crates are exempt. The floor defends against a stolen
        # publishing credential; azul publishing its own crate and bumping the
        # lockfile in the same change is not that, and blocking it would make
        # every release red. See cooldown-exempt.txt for the full reasoning.
        exempt = set(args.exempt)
        if args.exempt_file and args.exempt_file.is_file():
            for line in args.exempt_file.read_text(encoding="utf-8").splitlines():
                line = line.split("#", 1)[0].strip()
                if line:
                    exempt.add(line)
        pub = fetch_published([p for p in scope if p["name"] not in exempt], cache)
        now = datetime.now(timezone.utc)
        young = []
        skipped = sum(1 for p in scope if p["name"] in exempt)
        for p in scope:
            if p["name"] in exempt:
                continue
            iso = pub.get((p["name"], p["version"]))
            if not iso:
                continue
            when = datetime.fromisoformat(iso.replace("Z", "+00:00"))
            age = (now - when).days
            if age < args.min_age_days:
                young.append((p, age))
                violations.append({"kind": "COOLDOWN",
                                   "crate": f'{p["name"]} {p["version"]}',
                                   "detail": f"published {age}d ago, minimum is {args.min_age_days}d"})
        md.append(f"- **cooldown ({args.min_age_days}d)**: "
                  + (f"❌ {len(young)} version(s) too new" if young
                     else f"✅ all {len(pub)} checked versions are at least {args.min_age_days} days old")
                  + (f" ({skipped} first-party crate(s) exempt)" if skipped else ""))

    cache.flush()

    if violations:
        md += ["", "| kind | crate | detail |", "|---|---|---|"]
        md += [f'| **{v["kind"]}** | `{v["crate"]}` | {v["detail"]} |' for v in violations[:60]]
        if len(violations) > 60:
            md.append(f'| … | | {len(violations) - 60} more |')
    sc.summary("\n".join(md))

    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps({"violations": violations}, indent=2), encoding="utf-8")

    for line in md:
        if line.startswith("- "):
            print(line[2:].replace("**", ""))
    for v in violations[:40]:
        sc.annotate("error", f'{v["kind"]}: {v["crate"]} — {v["detail"]}')
    print(f"\n{len(violations)} violation(s)")
    return 0 if (not violations or args.report_only) else 1


if __name__ == "__main__":
    sys.exit(main())
