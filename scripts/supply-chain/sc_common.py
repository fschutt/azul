"""
sc_common.py — shared plumbing for azul's supply-chain gates.

Three scripts sit on top of this module:

    scan_build_scripts.py   which crates may run code at BUILD time, pinned by digest
    env_guard.py            which environment variables that code may READ
    lockfile_guard.py       which crate VERSIONS may enter the tree at all

They all need the same four things, so they live here rather than being
copy-pasted three ways: a vendor-directory walker, a content digest, a
GitHub-Actions summary/annotation emitter, and a TOML *subset* reader.

WHY A HAND-WRITTEN TOML READER. `tomllib` is Python 3.11+. The GitHub runner
image for ubuntu-22.04 ships Python 3.10, and stock macOS ships 3.9 — so a
`import tomllib` at the top of a CI gate is a gate that does not run. The
existing `scripts/check_dep_justifications.py` made the same call for the same
reason. The policy files in this directory are therefore written in a
deliberately small, fully-documented TOML subset (see `read_toml_subset`), which
is still real TOML — `taplo`, editors and `tomllib` all parse these files
correctly. The subset is a constraint on what we WRITE, not on what is valid.
"""
from __future__ import annotations

import hashlib
import os
import re
import sys
from pathlib import Path

# --------------------------------------------------------------------------
# TOML subset reader
# --------------------------------------------------------------------------
#
# Supported, and nothing else:
#
#     # comment
#     [table]  |  [table."quoted.key"]  |  [a.b.c]
#     key = "string"          (with \" and \\ escapes)
#     key = true | false
#     key = 123
#     key = ["one", "two"]    (single-line array of strings)
#
# Anything else raises, loudly, naming the file and line — a policy file that
# silently half-parses is worse than one that fails to parse, because the half
# that vanished is the half that was gating something.

_TBL_RE = re.compile(r'^\[([^\]]+)\]\s*(?:#.*)?$')
_KV_RE = re.compile(r'^([A-Za-z0-9_.-]+)\s*=\s*(.+?)\s*$')
_STR_RE = re.compile(r'^"((?:[^"\\]|\\.)*)"\s*(?:#.*)?$')
_ARR_RE = re.compile(r'^\[\s*(.*?)\s*,?\s*\]\s*(?:#.*)?$')
_ARR_ITEM_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')
_INT_RE = re.compile(r'^(-?\d+)\s*(?:#.*)?$')
_BOOL_RE = re.compile(r'^(true|false)\s*(?:#.*)?$')


def _unescape(s: str) -> str:
    return s.replace('\\"', '"').replace("\\\\", "\\").replace("\\n", "\n")


def _split_table_path(raw: str) -> list[str]:
    """`crate."serde_derive"` -> ['crate', 'serde_derive']; `a.b` -> ['a','b']."""
    parts, buf, in_q = [], "", False
    for ch in raw:
        if ch == '"':
            in_q = not in_q
        elif ch == "." and not in_q:
            parts.append(buf.strip())
            buf = ""
        else:
            buf += ch
    parts.append(buf.strip())
    return [p.strip('"') for p in parts if p.strip()]


def read_toml_subset(path: Path) -> dict:
    """Parse the documented TOML subset into nested dicts. Raises on anything
    outside the subset, naming file:line."""
    if not path.is_file():
        raise SystemExit(f"error: policy file not found: {path}")
    root: dict = {}
    cur = root
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = _TBL_RE.match(line)
        if m:
            cur = root
            for part in _split_table_path(m.group(1)):
                cur = cur.setdefault(part, {})
            continue
        m = _KV_RE.match(line)
        if not m:
            raise SystemExit(f"{path}:{lineno}: not a key/value or table header: {raw!r}")
        key, val = m.group(1), m.group(2)
        if (sm := _STR_RE.match(val)):
            cur[key] = _unescape(sm.group(1))
        elif (bm := _BOOL_RE.match(val)):
            cur[key] = bm.group(1) == "true"
        elif (im := _INT_RE.match(val)):
            cur[key] = int(im.group(1))
        elif (am := _ARR_RE.match(val)):
            cur[key] = [_unescape(x) for x in _ARR_ITEM_RE.findall(am.group(1))]
        else:
            raise SystemExit(
                f"{path}:{lineno}: value outside the supported TOML subset "
                f"(string / bool / int / single-line string array): {val!r}"
            )
    return root


# --------------------------------------------------------------------------
# Vendor directory
# --------------------------------------------------------------------------

_VERSIONED_DIR_RE = re.compile(r'^(?P<name>.+)-(?P<version>\d+\.\d+\.\d+(?:[-+].*)?)$')


class VendoredCrate:
    """One `name-version/` directory produced by `cargo vendor --versioned-dirs`."""

    __slots__ = ("name", "version", "path")

    def __init__(self, name: str, version: str, path: Path):
        self.name, self.version, self.path = name, version, path

    @property
    def id(self) -> str:
        return f"{self.name} {self.version}"

    def __repr__(self) -> str:
        return f"<VendoredCrate {self.id}>"


def walk_vendor(vendor_dir: Path) -> list[VendoredCrate]:
    """Every crate in the vendor directory, sorted by name then version.

    The vendor directory — not `cargo metadata` — is the source of truth for
    these gates on purpose. `cargo metadata` resolves FEATURES, so a crate that
    only compiles under a feature nobody enabled today is absent from it; the
    lockfile (and therefore `cargo vendor`) contains it anyway, and a feature
    flip in a later commit is enough to start compiling its build script. The
    strictly larger set is the correct one to review.
    """
    if not vendor_dir.is_dir():
        raise SystemExit(
            f"error: vendor directory not found: {vendor_dir}\n"
            f"       run: cargo vendor --locked --versioned-dirs {vendor_dir}"
        )
    # Resolve ONCE, here, so every crate path downstream is absolute.
    # `_follow_includes` resolves the files it discovers, so with a relative
    # --vendor the two halves disagree and `p.relative_to(crate.path)` raises
    # `'/abs/vendor/x/build.rs' is not in the subpath of 'vendor/x'`. Local runs
    # passed an absolute path and never hit it; CI passes `--vendor vendor` and
    # died on the first crate. Resolving here fixes the digest, the capability
    # scan and the checksum verifier at once, and does not change any digest:
    # those hash paths RELATIVE to the crate root, which is unaffected.
    vendor_dir = vendor_dir.resolve()
    out = []
    for entry in sorted(vendor_dir.iterdir()):
        if not entry.is_dir() or entry.name.startswith("."):
            continue
        m = _VERSIONED_DIR_RE.match(entry.name)
        if m:
            out.append(VendoredCrate(m.group("name"), m.group("version"), entry))
        else:
            # Not `--versioned-dirs`, or a crate whose version isn't semver-ish.
            # Recover the version from the manifest rather than dropping it: a
            # crate silently skipped by a security scan is a hole in the scan.
            ver = "0.0.0-unknown"
            mani = entry / "Cargo.toml"
            if mani.is_file():
                vm = re.search(r'(?m)^\s*version\s*=\s*"([^"]+)"', mani.read_text(
                    encoding="utf-8", errors="replace"))
                if vm:
                    ver = vm.group(1)
            out.append(VendoredCrate(entry.name, ver, entry))
    return out


def manifest_facts(crate: VendoredCrate) -> dict:
    """`build` / `links` / `proc-macro` straight out of the vendored Cargo.toml.

    Regex rather than a TOML parse: vendored manifests are cargo-normalised
    output, and we only need four scalars out of them. `build` is the subtle
    one — `build = false` means "no build script EVEN IF build.rs exists on
    disk", and `build = "custom.rs"` means the script is not called build.rs.
    Both shapes appear in the wild, and both are ways to make a naive
    `ls build.rs` scan report the wrong answer.
    """
    text = (crate.path / "Cargo.toml").read_text(encoding="utf-8", errors="replace") \
        if (crate.path / "Cargo.toml").is_file() else ""
    facts = {"build": None, "build_disabled": False, "links": None, "proc_macro": False}

    if (bm := re.search(r'(?m)^\s*build\s*=\s*(?:"([^"]+)"|(false)|(true))', text)):
        if bm.group(2):
            facts["build_disabled"] = True
        elif bm.group(1):
            facts["build"] = bm.group(1)
    if (lm := re.search(r'(?m)^\s*links\s*=\s*"([^"]+)"', text)):
        facts["links"] = lm.group(1)
    if re.search(r'(?m)^\s*proc-macro\s*=\s*true', text):
        facts["proc_macro"] = True
    return facts


# `mod name;`, optionally preceded by `#[path = "..."]`, and `include!("...")`.
_MOD_RE = re.compile(
    r'(?:#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]\s*)?'
    r'(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;')
_INCLUDE_RE = re.compile(r'include(?:_str|_bytes)?!\s*\(\s*"([^"]+)"\s*\)')


def _module_candidates(from_file: Path, path_attr: str | None, name: str) -> list[Path]:
    """Where rustc would look for `mod name;` declared inside `from_file`.

    Deliberately over-inclusive: every plausible resolution is offered and the
    caller keeps the ones that exist. Missing a file here means hashing less
    than what executes, which is the failure that matters; hashing one extra
    file that happens to exist costs nothing.
    """
    base = from_file.parent
    stem_dir = base if from_file.name in ("build.rs", "mod.rs", "main.rs", "lib.rs") \
        else base / from_file.stem
    if path_attr:
        return [base / path_attr, stem_dir / path_attr]
    return [base / f"{name}.rs", base / name / "mod.rs",
            stem_dir / f"{name}.rs", stem_dir / name / "mod.rs"]


def _follow_includes(crate_root: Path, seeds: list[Path]) -> tuple[list[Path], list[str]]:
    """Transitive closure of `mod`/`#[path]`/`include!` from the seed files.

    A build script that is one file today can become two in the next release —
    `libm` keeps `env::vars()` in a root-level `configure.rs`, `portable-atomic`
    pulls in `src/gen/build.rs` and `version.rs`. Without following the graph,
    a digest over `build.rs` alone pins the door and leaves the window open, and
    a capability scan reports "no environment access" for a crate that sweeps
    the whole environment one `mod` declaration away.

    Returns (files, escapes) where `escapes` names any reference resolving
    OUTSIDE the crate directory — which a vendored crate has no legitimate
    reason to do, and which the caller should surface rather than silently drop.
    """
    seen: set[Path] = set()
    escapes: list[str] = []
    stack = list(seeds)
    while stack:
        f = stack.pop()
        try:
            f = f.resolve()
        except OSError:
            continue
        if f in seen or not f.is_file():
            continue
        seen.add(f)
        if f.suffix != ".rs":
            continue
        try:
            text = f.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        refs: list[Path] = []
        for path_attr, name in _MOD_RE.findall(text):
            refs.extend(_module_candidates(f, path_attr or None, name))
        for inc in _INCLUDE_RE.findall(text):
            if "$" in inc or "concat!" in inc:   # OUT_DIR-relative, generated
                continue
            refs.append(f.parent / inc)
        for r in refs:
            try:
                rr = r.resolve()
            except OSError:
                continue
            if not rr.is_file():
                continue
            try:
                rr.relative_to(crate_root.resolve())
            except ValueError:
                escapes.append(str(r))
                continue
            stack.append(rr)
    return sorted(seen), sorted(set(escapes))


def build_script_paths(crate: VendoredCrate) -> list[Path]:
    """Every file that runs at BUILD time for this crate, relative-sorted.

    That is the declared build script, a sibling `build/` module directory
    (clang-sys, openssl-sys, thiserror and rustversion all split their build
    script across one), AND the transitive closure of everything the script
    pulls in with `mod` / `#[path]` / `include!`. Hashing only `build.rs` would
    let a compromised release move the payload one file sideways and keep the
    pin green — which is not hypothetical: 16 of the 141 build scripts in
    azul's tree already span more than one file.
    """
    facts = manifest_facts(crate)
    if facts["build_disabled"]:
        return []
    paths: list[Path] = []
    declared = facts["build"]
    if declared:
        p = crate.path / declared
        if p.is_file():
            paths.append(p)
    elif (crate.path / "build.rs").is_file():
        paths.append(crate.path / "build.rs")
    if not paths:
        return []
    build_dir = crate.path / "build"
    if build_dir.is_dir():
        paths.extend(sorted(q for q in build_dir.rglob("*") if q.is_file()))
    followed, _escapes = _follow_includes(crate.path, paths)
    return sorted(set(paths) | set(followed))


def build_script_escapes(crate: VendoredCrate) -> list[str]:
    """References from build-time code that resolve outside the crate directory."""
    facts = manifest_facts(crate)
    if facts["build_disabled"]:
        return []
    seeds = []
    if facts["build"] and (crate.path / facts["build"]).is_file():
        seeds.append(crate.path / facts["build"])
    elif (crate.path / "build.rs").is_file():
        seeds.append(crate.path / "build.rs")
    if not seeds:
        return []
    return _follow_includes(crate.path, seeds)[1]


def proc_macro_paths(crate: VendoredCrate) -> list[Path]:
    """Sources of a proc-macro crate — these run INSIDE rustc, with the same
    ambient authority a build script has and none of the visibility."""
    src = crate.path / "src"
    return sorted(p for p in src.rglob("*.rs")) if src.is_dir() else []


def digest_files(root: Path, paths: list[Path]) -> str:
    """sha256 over (relative path, content) pairs — order-independent, and
    sensitive to a file being ADDED or RENAMED, not just edited.

    Line endings are normalised to \\n so a crate that ships CRLF in one release
    and LF in the next does not read as a payload change. Nothing else is
    normalised: whitespace and comments are part of what was reviewed.
    """
    h = hashlib.sha256()
    for p in sorted(paths, key=lambda q: str(q.relative_to(root)).replace(os.sep, "/")):
        rel = str(p.relative_to(root)).replace(os.sep, "/")
        data = p.read_bytes().replace(b"\r\n", b"\n")
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(hashlib.sha256(data).digest())
    return h.hexdigest()


# --------------------------------------------------------------------------
# GitHub Actions output
# --------------------------------------------------------------------------

def summary(text: str) -> None:
    """Append markdown to the run Summary, when running under Actions."""
    dest = os.environ.get("GITHUB_STEP_SUMMARY")
    if dest:
        with open(dest, "a", encoding="utf-8") as fh:
            fh.write(text.rstrip("\n") + "\n")


def annotate(level: str, message: str) -> None:
    """`::error::`/`::warning::`/`::notice::` — surfaced on the run page.

    Newlines are encoded (%0A) because a raw newline TERMINATES a workflow
    command: a multi-line annotation printed naively loses everything after the
    first line, which on a security gate is the part naming the offender.
    """
    if os.environ.get("GITHUB_ACTIONS"):
        print(f"::{level}::" + message.replace("%", "%25")
              .replace("\r", "%0D").replace("\n", "%0A"))
    else:
        print(f"[{level.upper()}] {message}", file=sys.stderr)


def rel_to_repo(p: Path) -> str:
    try:
        return str(p.relative_to(Path.cwd()))
    except ValueError:
        return str(p)


# --------------------------------------------------------------------------
# Build-dependency closure
# --------------------------------------------------------------------------

def build_dependency_closure(metadata: dict) -> set[str]:
    """Names of every crate that is, transitively, a `[build-dependencies]` entry.

    These crates have no `build.rs` and are not proc macros, so a scan scoped to
    "build scripts and proc macros" never looks at them — yet their library code
    executes at build time with exactly the same authority, because a build
    script is a program and these are the libraries it links.

    This is not a theoretical hole. `built` 0.7.7 reaches azul as a build
    dependency of `turso_core`; it calls `env::vars_os()` over the whole
    environment and uses `git2::Repository::discover` to walk UPWARD out of the
    crate directory, baking the enclosing repository's branch, commit hash and
    dirty flag into the compiled artifact. `turso_core`'s own build.rs is twenty
    lines and does none of that.

    Takes parsed `cargo metadata --format-version=1` output. Walks the resolve
    graph from every edge whose `dep_kinds` contains `build`, then takes the
    full normal-dependency closure of each — a build dependency's own
    dependencies also run at build time.
    """
    resolve = metadata.get("resolve") or {}
    nodes = {n["id"]: n for n in resolve.get("nodes", [])}
    id_to_name = {p["id"]: p["name"] for p in metadata.get("packages", [])}

    seeds: set[str] = set()
    for node in nodes.values():
        for dep in node.get("deps", []):
            kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
            if "build" in kinds:
                seeds.add(dep["pkg"])

    closure: set[str] = set()
    stack = list(seeds)
    while stack:
        pid = stack.pop()
        if pid in closure:
            continue
        closure.add(pid)
        for dep in nodes.get(pid, {}).get("deps", []):
            if dep["pkg"] not in closure:
                stack.append(dep["pkg"])
    return {id_to_name[p] for p in closure if p in id_to_name}


def load_metadata(path: Path | None = None) -> dict:
    """`cargo metadata` output, from a file if given else by invoking cargo."""
    import json
    import subprocess
    if path and path.is_file():
        return json.loads(path.read_text(encoding="utf-8"))
    out = subprocess.run(
        ["cargo", "metadata", "--format-version=1", "--locked", "--all-features"],
        capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit(f"error: cargo metadata failed:\n{out.stderr[-2000:]}")
    return json.loads(out.stdout)


# --------------------------------------------------------------------------
# Source normalisation
# --------------------------------------------------------------------------

_COMMENT_LINE_RE = re.compile(r'//[^\n]*')
_COMMENT_BLOCK_RE = re.compile(r'/\*.*?\*/', re.S)
_TEST_ATTR_RE = re.compile(
    r'#\s*\[\s*(?:'
    r'test'
    r'|cfg\s*\(\s*test\s*\)'
    r'|cfg\s*\([^)]*\btest\b[^)]*\)'
    r'|cfg_attr\s*\(\s*test\s*,[^)]*\)'
    r'|(?:tokio|async_std|rstest|proptest|quickcheck)\s*::\s*\w+'
    r'|bench'
    r')\s*\]')


def _skip_to_item_body(text: str, start: int) -> int:
    """From just past an attribute, find the `{` opening the item it applies to,
    or the `;` ending it. Returns the index one past the item."""
    i = start
    n = len(text)
    while i < n:
        c = text[i]
        if c == "#":                       # another attribute — keep scanning
            i += 1
            continue
        if c == ";":                       # `#[test] fn f();` — item has no body
            return i + 1
        if c == "{":
            break
        i += 1
    if i >= n:
        return n
    depth, in_str, in_chr, esc = 0, False, False, False
    while i < n:
        c = text[i]
        if esc:
            esc = False
        elif c == "\\" and (in_str or in_chr):
            esc = True
        elif in_str:
            if c == '"':
                in_str = False
        elif in_chr:
            if c == "'":
                in_chr = False
        elif c == '"':
            in_str = True
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def strip_test_items(text: str) -> str:
    """Remove `#[test]` / `#[cfg(test)]` items.

    Test code is not build-time code: rustc only compiles these items under
    `--test`, so a `cargo build` never runs them. Leaving them in makes the
    scanners report reads that cannot happen — `security-framework`'s
    `authorization.rs` reads `PASSWORD` and `USER` in three `#[test]` functions
    and one test helper, which is a hard failure on a credential-shaped name
    for code that is never compiled into anything azul builds.

    Skipping them is safe in the other direction too: a payload hidden behind
    `#[cfg(test)]` genuinely does not execute during a dependency's build.
    """
    out, last = [], 0
    for m in _TEST_ATTR_RE.finditer(text):
        if m.start() < last:
            continue
        end = _skip_to_item_body(text, m.end())
        out.append(text[last:m.start()])
        last = end
    out.append(text[last:])
    return "".join(out)


def normalise_source(text: str) -> str:
    """Comments out, test items out — what is left is what runs at build time."""
    text = _COMMENT_BLOCK_RE.sub("", _COMMENT_LINE_RE.sub("", text))
    return strip_test_items(text)
