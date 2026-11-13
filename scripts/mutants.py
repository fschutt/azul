#!/usr/bin/env python3
"""mutants.py - the replay of the patch mutants (ARCH31-1; notes/audits/closeout-2026-11/arch29/ARCH29.md 9.10.3).

Why: a test that cannot fail proves nothing, and a fix commit used to carry its evidence as a sentence ("RED not run", "I
removed the guard by hand and the test went red"). Now a commit that closes a MAJOR or BLOCKER row of the register adds
`iso/mutants/<row id>.patch` (azul: `mutants/<row id>.patch`): the few lines that turn the fix's guard OFF, with the tests
that must notice (its KILLERS), or it says `Mutant: none (<reason>)` for a reviewer to read. This script is the machine
that checks the claim: with the patch applied every killer must go RED, without it GREEN.

    mutants.sh --patch FILE     replay one patch (on the Mac: INSIDE ONE `cg` lock hold, one patch at a time)
    mutants.sh --range A..B     replay every patch added or changed in A..B        (Linux CI; --many elsewhere)
    mutants.sh --all            replay every patch of the folder                   (Linux CI; --many elsewhere)
    mutants.sh --ci SPEC        what the workflow runs: SPEC is `all` or `A..B`; the apply check of EVERY patch first
    mutants.sh --patches        list the patches with their rows and killers
    mutants.sh --recover        undo the patch an interrupted replay (a SIGKILL, a reboot) left in the tree, from its journal
    ... --dry-run               no build, no change to the tree: the header, the static checks and `git apply --check`
                                against HEAD (a temporary index: the working tree may be anything)

A patch file starts with a header of comment lines, then the unified diff (`git apply` skips the comments):

    # row: X-DELTA-FOO-3                       the register row(s) this patch is the evidence of (`, ` between)
    # why: what the guard is and what the patch does to it, one line
    # killers: azinit:s3d::repair::tests::x_delta_foo_3_a_request_is_kept, azlin-token:x_delta_foo_3_the_poll
    #   azctl+dev:ctl::fleet::tests::a_third      (a continuation line starts with `#` and three spaces)

A killer is `<crate>[+<feature>...]:<filter>`, run as `cargo test -p <crate> [--features <feature,..>] --lib -- <filter>`; a
filter that holds `::` is a test's full path inside the crate's lib test binary and is run with `--exact`, any other filter
is cargo's substring match. azinit and azctl get `--features dev` unless a `+feature` says otherwise. Lib tests only.

What one replay does (the safe mode, always on, so a lane on the shared checkout and a clean CI checkout run the same code):
  1. the header, the patch text (no conflict marker, no rename, no binary), `git apply --check` against HEAD;
  2. REFUSES when any file the patch touches is modified in the working tree (another lane's hunk, or an interrupted replay:
     the message says which and what undoes it); the tree is untouched at this point;
  3. the CONTROL: every killer on the tree as it is must be green and must run at least one test (a filter that matches no
     test is `NOKILLER`, never a pass; a killer that is red without the patch proves nothing: `CONTROL-RED`);
  4. applies the patch, builds (a build that breaks is `NOBUILD`: a compile error is no kill), runs every killer: each must
     FAIL with a failed test; green = `SURVIVED`. A crash, a timeout or no test summary is `INCONCLUSIVE`, never a kill;
  5. reverses the patch (also on SIGINT, SIGTERM, SIGHUP and on an error) and PROVES the tree is back: the content hash of
     every touched file equals the one before the apply, `git status` and `git diff --stat` of those files are empty, the
     files' mtimes are bumped (so cargo cannot reuse the mutant's build). A restore that cannot be proved is `RESTORE-FAILED`.
A SIGKILL or a reboot cannot be caught: the replay therefore writes a JOURNAL before it applies the patch
(`<git dir>/mutants-journal/<patch name>.json`: the patch, the files, their hashes before, the pid) and removes it only after
the proved restore. While a journal exists NO replay starts (it says which patch is still applied); `--recover` reverses
that patch from the journal, proves the files equal their hashes before, bumps their mtimes and removes the journal.

Exit codes of --patch (several patches: the highest): 0 KILLED (or APPLIES under --dry-run), 1 SURVIVED, 2 NOAPPLY / NOBUILD
(the patch is unusable: it does not apply, is malformed, touches a modified file, or breaks the build), 3 NOKILLER or
CONTROL-RED (a killer matches no test, or is red without the patch), 4 INCONCLUSIVE, 5 RESTORE-FAILED, 70 an error of this
script. The last lines are one `RESULT<TAB>verdict<TAB>row<TAB>patch<TAB>detail` line per patch (the register tool and CI read
them). A SURVIVED patch means the register row is NOT closed: the coordinator reopens it with
`issues_status.py --set <row>=OPEN`. The replay costs two builds of the killers' crates per patch: never run several on the Mac.

The same file lives in azul-apps (iso/scripts/mutants.py) and in azul (scripts/mutants.py); keep them byte-identical.
Python 3.9 or later, standard library only.
"""
from __future__ import annotations

import argparse
import contextlib
import dataclasses
import hashlib
import json
import os
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path
from typing import Callable, Dict, List, Optional, Tuple

VERDICT_EXIT = {
    "KILLED": 0,
    "APPLIES": 0,
    "SURVIVED": 1,
    "NOAPPLY": 2,
    "NOBUILD": 2,
    "NOKILLER": 3,
    "CONTROL-RED": 3,
    "INCONCLUSIVE": 4,
    "RESTORE-FAILED": 5,
}
INTERNAL_ERROR = 70
# azinit and azctl compile their tests with the `dev` feature (CI: `cargo test --workspace --features azctl/dev,azinit/dev`)
DEFAULT_FEATURES = {"azinit": ("dev",), "azctl": ("dev",)}

HEADER_KEY = re.compile(r"^#[ \t]?([a-z][a-z-]*):[ \t]*(.*?)[ \t]*$")
# the keys a header may carry: `row`, `why` and `killers` are read; the others are prose for the reader of the patch. Any other
# word before a colon is a mistake (a continuation line with fewer than three spaces looks like a key) and is refused, so a
# killer is never dropped without a word.
HEADER_KEYS = ("row", "why", "killers", "mutant", "guard", "replay", "apply", "note")
HEADER_MORE = re.compile(r"^#[ \t]{3,}(\S.*?)[ \t]*$")
KILLER = re.compile(
    r"^(?P<crate>[A-Za-z0-9][A-Za-z0-9_-]*)(?P<feat>(?:\+[A-Za-z0-9][A-Za-z0-9_./-]*)*):(?P<filter>[^\s,:][^\s,]*)$"
)
ROW_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]*$")
NOT_A_MUTANT = re.compile(r"^(rename (from|to) |copy (from|to) |similarity index |dissimilarity index |old mode |new mode |GIT binary patch|Binary files )")
CONFLICT_ADDED = re.compile(r"^\+(<{7}|={7}|>{7})(\s|$)")
CONFLICT_FILE = re.compile(r"^(<{7}|>{7})( |$)", re.M)
RESULT_LINE = re.compile(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", re.M)
FAILED_TEST = re.compile(r"^test (\S+) \.\.\. FAILED$", re.M)
STDOUT_BLOCK = re.compile(r"^---- (\S+) stdout ----$", re.M)
RANGE_SIDE = re.compile(r"^[A-Za-z0-9_./~^@{}+-]+$")


class Interrupted(BaseException):
    """A caught SIGINT / SIGTERM / SIGHUP: unwinds through the `finally` that reverses the patch."""

    def __init__(self, code: int):
        super().__init__(code)
        self.code = code


class Refusal(Exception):
    """A verdict known without a build."""

    def __init__(self, verdict: str, detail: str):
        super().__init__(detail)
        self.verdict, self.detail = verdict, detail


# --- the header ---------------------------------------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Killer:
    crate: str
    features: Tuple[str, ...]
    given: Tuple[str, ...]  # the features the header named (the default ones are not repeated in the display)
    filter: str

    @property
    def exact(self) -> bool:
        return "::" in self.filter

    def spec(self) -> str:
        return self.crate + "".join("+" + f for f in self.given) + ":" + self.filter

    def key(self) -> Tuple[str, Tuple[str, ...]]:
        return (self.crate, self.features)

    def build_args(self) -> List[str]:
        args = ["test", "-p", self.crate]
        if self.features:
            args += ["--features", ",".join(self.features)]
        return args + ["--lib"]

    def run_args(self) -> List[str]:
        return self.build_args() + ["--", self.filter] + (["--exact"] if self.exact else [])


@dataclasses.dataclass
class Header:
    rows: List[str]
    why: str
    killers: List[Killer]
    problems: List[str]


def parse_killer(token: str) -> Optional[Killer]:
    m = KILLER.match(token)
    if not m:
        return None
    given = tuple(f for f in m.group("feat").split("+") if f)
    features = given or DEFAULT_FEATURES.get(m.group("crate"), ())
    return Killer(m.group("crate"), features, given, m.group("filter"))


def parse_header(text: str) -> Header:
    """The `# key: value` comment lines at the top of a patch file (a line of three or more spaces after `#` continues the
    key above it). Problems are collected, not raised: the caller refuses the patch with all of them in one message."""
    fields: Dict[str, List[str]] = {}
    key: Optional[str] = None
    unknown: List[str] = []
    for line in text.splitlines():
        if not line.startswith("#"):
            break
        m = HEADER_KEY.match(line)
        if m:
            key = m.group(1)
            if key not in HEADER_KEYS and key not in unknown:
                unknown.append(key)
            fields.setdefault(key, []).append(m.group(2))
            continue
        more = HEADER_MORE.match(line)
        if more and key:
            fields[key].append(more.group(1))
        else:
            key = None
    problems: List[str] = []
    for u in unknown:
        problems.append(
            f"`# {u}:` is no header key ({', '.join(HEADER_KEYS)}); a continuation line of `# killers:` or `# row:` starts with `#` and three spaces"
        )
    rows = [t for t in re.split(r"[,\s]+", " ".join(fields.get("row", []))) if t]
    if not rows:
        problems.append("no `# row: <register id>` line")
    for r in rows:
        if not ROW_ID.match(r):
            problems.append(f"`# row:` names `{r}`, which is no register id")
    killers: List[Killer] = []
    tokens = [t for t in re.split(r"[,\s]+", " ".join(fields.get("killers", []))) if t]
    if not tokens:
        problems.append("no `# killers: <crate>:<test filter>[, ...]` line")
    for t in tokens:
        k = parse_killer(t)
        if k is None:
            problems.append(f"killer `{t}` is not `<crate>[+<feature>]:<filter>` (one colon after the crate)")
        else:
            killers.append(k)
    why = " ".join(fields.get("why", [])).strip()
    return Header(rows, why, killers, problems)


# --- what cargo says ----------------------------------------------------------------------------------------------


@dataclasses.dataclass
class Run:
    rc: Optional[int]  # None: killed by the timeout
    text: str
    seconds: float
    results: int = 0  # `test result:` lines
    passed: int = 0
    failed: int = 0
    ignored: int = 0
    first_failure: str = ""

    @property
    def ran(self) -> int:
        return self.passed + self.failed


def parse_run(rc: Optional[int], text: str, seconds: float) -> Run:
    run = Run(rc, text, seconds)
    for m in RESULT_LINE.finditer(text):
        run.results += 1
        run.passed += int(m.group(1))
        run.failed += int(m.group(2))
        run.ignored += int(m.group(3))
    block = STDOUT_BLOCK.search(text)
    if block:
        kept: List[str] = []
        for line in text[block.end():].splitlines():
            if line.startswith("note: run with") or line.startswith("failures:") or line.startswith("---- "):
                break
            if line.strip():
                kept.append(line.strip()[:160])
            if len(kept) == 5:
                break
        run.first_failure = f"{block.group(1)}: " + " | ".join(kept)
    elif run.failed:
        names = FAILED_TEST.findall(text)
        run.first_failure = f"{names[0]}: FAILED" if names else "a test FAILED"
    return run


def tail(text: str, n: int = 20) -> str:
    lines = [ln for ln in text.splitlines() if ln.strip()]
    return "\n".join("    | " + ln[:200] for ln in lines[-n:])


# --- the context, git, children ---------------------------------------------------------------------------------------


@dataclasses.dataclass
class Ctx:
    root: Path
    workdir: Path
    patch_dir: Path
    cargo: List[str]
    env: Dict[str, str]
    test_timeout: int
    build_timeout: Optional[int]
    log_dir: Optional[Path]
    many_ok: bool
    say: Callable[[str], None]


def git_env(extra: Optional[Dict[str, str]] = None) -> Dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE")}
    env["GIT_OPTIONAL_LOCKS"] = "0"  # a status here never takes the index lock another lane's commit needs
    env.update(extra or {})
    return env


def git(root: Path, *args: str, env: Optional[Dict[str, str]] = None) -> "subprocess.CompletedProcess[str]":
    return subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True, errors="replace", env=git_env(env), cwd=str(root)
    )


def kill_group(p: "subprocess.Popen[str]") -> None:
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(p.pid, sig)
        except (ProcessLookupError, PermissionError):
            return
        try:
            p.wait(timeout=5)
            return
        except subprocess.TimeoutExpired:
            continue


def run_cmd(cmd: List[str], cwd: Path, env: Dict[str, str], timeout: Optional[int]) -> Tuple[Optional[int], str]:
    """(exit status, output): the status is None when the timeout killed the process group. A signal for this script kills the
    child's group first, so no build outlives the lock hold that holds it."""
    p = subprocess.Popen(
        cmd, cwd=str(cwd), env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace",
        start_new_session=True,
    )
    try:
        out, _ = p.communicate(timeout=timeout or None)
        return p.returncode, out or ""
    except subprocess.TimeoutExpired:
        kill_group(p)
        out, _ = p.communicate()
        return None, out or ""
    except BaseException:
        kill_group(p)
        raise


@contextlib.contextmanager
def signals_caught():
    """SIGINT, SIGTERM and SIGHUP become `Interrupted`, so a `finally` runs; the old handlers come back after."""

    def handler(signum, _frame):
        raise Interrupted(128 + signum)

    old = {}
    try:
        for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            old[s] = signal.signal(s, handler)
    except ValueError:  # not the main thread (a test harness): nothing to catch
        old = {}
    try:
        yield
    finally:
        for s, h in old.items():
            signal.signal(s, h)


# --- one patch ------------------------------------------------------------------------------------------------------


@dataclasses.dataclass
class Info:
    path: Path
    rel: str
    header: Header
    touched: List[str]

    @property
    def row(self) -> str:
        return ",".join(self.header.rows) or "-"


@dataclasses.dataclass
class Result:
    verdict: str
    row: str
    patch: str
    detail: str
    seconds: float = 0.0

    def line(self) -> str:
        clean = lambda s: re.sub(r"[\t\r\n]+", " ", s).strip()
        return "\t".join(["RESULT", self.verdict, clean(self.row), clean(self.patch), clean(self.detail)])

    @property
    def code(self) -> int:
        return VERDICT_EXIT[self.verdict]


def rel_to_root(root: Path, path: Path) -> str:
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        return str(path)


def numstat(root: Path, path: Path) -> List[str]:
    done = git(root, "apply", "--numstat", "-z", str(path))
    if done.returncode != 0:
        raise Refusal("NOAPPLY", "git cannot read the patch: " + (done.stderr.strip().splitlines() or ["?"])[0])
    parts = done.stdout.split("\0")
    touched: List[str] = []
    i = 0
    while i < len(parts) and parts[i]:
        added, deleted, name = (parts[i].split("\t", 2) + ["", ""])[:3]
        if name == "" or added == "-" or deleted == "-":  # a rename (`-z` leaves the name empty) or a binary change
            raise Refusal("NOAPPLY", "a rename, copy or binary change is no mutant")
        if name.startswith("/") or ".." in Path(name).parts or name.startswith(".git/"):
            raise Refusal("NOAPPLY", f"{name}: a path outside the work tree")
        touched.append(name)
        i += 1
    if not touched:
        raise Refusal("NOAPPLY", "the patch changes no file")
    return touched


def load_patch(ctx: Ctx, path: Path) -> Info:
    rel = rel_to_root(ctx.root, path)
    if not path.is_file():
        raise Refusal("NOAPPLY", "no such patch file")
    try:
        text = path.read_bytes().decode("utf-8")
    except UnicodeDecodeError:
        raise Refusal("NOAPPLY", "the patch is not UTF-8")
    header = parse_header(text)
    if header.problems:
        raise Refusal("NOAPPLY", "header: " + "; ".join(header.problems))
    for n, line in enumerate(text.splitlines(), 1):
        if NOT_A_MUTANT.match(line):  # no line of a hunk starts like this: a hunk line begins with a space, `+`, `-` or `@`
            raise Refusal("NOAPPLY", f"patch line {n} (`{line[:40]}`): a mutant changes lines of a file, no rename, copy, mode or binary change")
        if CONFLICT_ADDED.match(line):
            raise Refusal("NOAPPLY", f"the patch adds a conflict marker (patch line {n})")
    touched = numstat(ctx.root, path)
    if not re.search(r"^@@ ", text, re.M):
        raise Refusal("NOAPPLY", "the patch has no hunk (`@@`)")
    return Info(path, rel, header, touched)


def head_applies(ctx: Ctx, info: Info) -> str:
    """`git apply --check` of the patch against HEAD's tree through a temporary index, so it says what a clean checkout
    would say whatever the working tree and the real index hold. Returns the short id of HEAD."""
    head = git(ctx.root, "rev-parse", "--verify", "--quiet", "HEAD^{commit}")
    if head.returncode != 0:
        raise Refusal("NOAPPLY", "the repository has no HEAD")
    with tempfile.TemporaryDirectory(prefix="mutants-index-") as d:
        env = {"GIT_INDEX_FILE": os.path.join(d, "index")}
        if git(ctx.root, "read-tree", "HEAD", env=env).returncode != 0:
            raise Refusal("NOAPPLY", "git read-tree HEAD failed")
        done = git(ctx.root, "apply", "--cached", "--check", str(info.path), env=env)
    if done.returncode != 0:
        why = " / ".join(ln.strip() for ln in done.stderr.splitlines()[:2]) or "git apply --check failed"
        raise Refusal("NOAPPLY", f"does not apply to HEAD {head.stdout.strip()[:7]}: {why}")
    return head.stdout.strip()[:7]


def dirty(ctx: Ctx, paths: List[str]) -> List[str]:
    done = git(ctx.root, "status", "--porcelain=v1", "-z", "--untracked-files=all", "--", *paths)
    if done.returncode != 0:  # a status that failed is no "clean"
        raise Refusal("NOAPPLY", "git status failed: " + (done.stderr.strip().splitlines() or ["?"])[0])
    return [e[3:] for e in done.stdout.split("\0") if len(e) > 3]


def require_clean(ctx: Ctx, info: Info) -> None:
    bad = dirty(ctx, info.touched)
    if not bad:
        return
    left = git(ctx.root, "apply", "-R", "--check", str(info.path)).returncode == 0
    why = (
        "they carry this patch (an interrupted replay): `git apply -R " + info.rel + "` restores them, nothing is reversed here"
        if left
        else "another lane's hunk, or an interrupted replay: look at `git diff` of them first"
    )
    raise Refusal("NOAPPLY", f"refused, the working tree is not clean for {', '.join(bad[:4])}: {why}")


def snapshot(root: Path, paths: List[str]) -> Dict[str, Optional[Tuple[str, int]]]:
    out: Dict[str, Optional[Tuple[str, int]]] = {}
    for p in paths:
        f = root / p
        try:
            data = f.read_bytes()
            out[p] = (hashlib.sha256(data).hexdigest(), f.stat().st_mode & 0o111)
        except OSError:
            out[p] = None
    return out


def mtimes(root: Path, paths: List[str]) -> Dict[str, int]:
    out: Dict[str, int] = {}
    for p in paths:
        with contextlib.suppress(OSError):
            out[p] = (root / p).stat().st_mtime_ns
    return out


def restore(ctx: Ctx, info: Info, pre: Dict[str, Optional[Tuple[str, int]]], applied: Dict[str, int]) -> List[str]:
    """Reverse the patch when the files differ from before the apply, bump their mtimes, and PROVE the tree is back.
    Returns the problems (empty: proved). Never raises: it runs inside a `finally`."""
    problems: List[str] = []
    try:
        if snapshot(ctx.root, info.touched) != pre:
            done = git(ctx.root, "apply", "-R", str(info.path))
            if done.returncode != 0:
                msg = (done.stderr.strip().splitlines() or ["?"])[0]
                problems.append(f"`git apply -R {info.rel}` failed ({msg}): a file changed under the replay; nothing was overwritten")
        for p in info.touched:
            f = ctx.root / p
            if f.exists():
                floor = applied.get(p, 0) + 1_000_000
                t = max(time.time_ns(), floor)
                os.utime(f, ns=(t, t))
        after = snapshot(ctx.root, info.touched)
        if after != pre:
            changed = [p for p in info.touched if after.get(p) != pre.get(p)]
            problems.append("content differs from before the apply: " + ", ".join(changed))
        stat = git(ctx.root, "diff", "--stat", "--", *info.touched).stdout.strip()
        status = dirty(ctx, info.touched)
        if stat or status:
            problems.append("git still sees a change: " + ", ".join(status or [stat.splitlines()[0]]))
        for p in info.touched:
            if p in applied and (ctx.root / p).exists() and (ctx.root / p).stat().st_mtime_ns <= applied[p]:
                problems.append(f"{p}: the mtime was not bumped")
    except Exception as e:  # a proof that could not be made is no proof
        problems.append(f"the restore check failed: {e}")
    return problems


# --- the journal ------------------------------------------------------------------------------------------------------

JOURNAL = "mutants-journal"


def journal_dir(ctx: Ctx) -> Path:
    done = git(ctx.root, "rev-parse", "--absolute-git-dir")
    if done.returncode != 0 or not done.stdout.strip():
        raise Refusal("NOAPPLY", "git cannot say where its directory is: no journal can be kept, so nothing is applied")
    return Path(done.stdout.strip()) / JOURNAL


def journals(ctx: Ctx) -> List[Path]:
    d = journal_dir(ctx)
    return sorted(d.glob("*.json")) if d.is_dir() else []


def pid_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def journal_open(ctx: Ctx, info: Info, pre: Dict[str, Optional[Tuple[str, int]]]) -> Path:
    """Written (and synced) BEFORE the patch is applied, removed after the proved restore: a SIGKILL or a reboot between the two
    leaves the evidence `--recover` needs. O_EXCL: one journal per patch."""
    d = journal_dir(ctx)
    d.mkdir(parents=True, exist_ok=True)
    path = d / (re.sub(r"[^A-Za-z0-9_.-]+", "_", info.rel) + ".json")
    entry = {
        "patch": info.rel,
        "touched": info.touched,
        "pre": {p: list(v) if v else None for p, v in pre.items()},
        "pid": os.getpid(),
        "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    }
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    except FileExistsError:
        raise Refusal("NOAPPLY", f"a journal of this patch exists ({path}): `mutants.py --recover` first")
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump(entry, fh, indent=1)
        fh.flush()
        os.fsync(fh.fileno())
    return path


def refuse_while_journal(ctx: Ctx) -> None:
    """No replay starts while an earlier one may have left its patch applied: the control run would build on that mutant."""
    left: List[str] = []
    for j in journals(ctx):
        try:
            e = json.loads(j.read_text(encoding="utf-8"))
            who = f"{e['patch']} (pid {e.get('pid', '?')}, started {e.get('started', '?')})"
            if pid_alive(int(e.get("pid", 0))) and int(e.get("pid", 0)) != os.getpid():
                who += ", still running"
        except (OSError, ValueError, KeyError, TypeError):
            who = f"{j.name} (unreadable)"
        left.append(who)
    if left:
        raise Refusal("NOAPPLY", "refused, an earlier replay left its patch applied or is still running: " + "; ".join(left) + " -- `mutants.py --recover` reverses it")


def recover(ctx: Ctx) -> int:
    """Reverse what the journals say an interrupted replay left, prove the files equal their hashes before, remove the journal."""
    paths = journals(ctx)
    if not paths:
        ctx.say("mutants: no journal: no replay was interrupted, nothing to recover")
        return 0
    worst = 0
    for j in paths:
        try:
            e = json.loads(j.read_text(encoding="utf-8"))
            rel, touched, pid = e["patch"], list(e["touched"]), int(e.get("pid", 0))
            pre = {p: (tuple(v) if v else None) for p, v in e["pre"].items()}
        except (OSError, ValueError, KeyError, TypeError) as err:
            ctx.say(f"  {j.name}: unreadable ({err}); nothing was changed: look at `git status` and `git diff` by hand")
            worst = max(worst, VERDICT_EXIT["RESTORE-FAILED"])
            continue
        if pid_alive(pid) and pid != os.getpid():
            ctx.say(f"  {rel}: its replay is still running (pid {pid}): not touched")
            worst = max(worst, VERDICT_EXIT["INCONCLUSIVE"])
            continue
        info = Info(ctx.root / rel, rel, Header([], "", [], []), touched)
        problems = restore(ctx, info, pre, {})
        if problems:
            ctx.say(f"  {rel}: NOT recovered: " + "; ".join(problems) + " (the journal stays)")
            worst = max(worst, VERDICT_EXIT["RESTORE-FAILED"])
            continue
        j.unlink()
        ctx.say(f"  {rel}: recovered; proved: hashes equal to the journal's, git status and git diff --stat empty, mtimes bumped")
    return worst


# --- the replay -------------------------------------------------------------------------------------------------------


def cargo_run(ctx: Ctx, label: str, args: List[str], timeout: Optional[int]) -> Run:
    t0 = time.monotonic()
    rc, text = run_cmd(ctx.cargo + args, ctx.workdir, ctx.env, timeout)
    run = parse_run(rc, text, time.monotonic() - t0)
    if ctx.log_dir is not None:
        ctx.log_dir.mkdir(parents=True, exist_ok=True)
        name = re.sub(r"[^A-Za-z0-9_.-]+", "_", label)[:120]
        (ctx.log_dir / f"{name}.log").write_text("$ " + " ".join(ctx.cargo + args) + "\n" + text, encoding="utf-8")
    return run


def build_all(ctx: Ctx, killers: List[Killer], phase: str) -> Optional[str]:
    """Build each distinct (crate, features) test binary once; the detail of the first failure, or None."""
    seen = set()
    for k in killers:
        if k.key() in seen:
            continue
        seen.add(k.key())
        run = cargo_run(ctx, f"{phase}-build-{k.crate}", k.build_args() + ["--no-run"], ctx.build_timeout)
        if run.rc != 0:
            ctx.say(tail(run.text))
            how = "timed out" if run.rc is None else f"exit {run.rc}"
            return f"`cargo {' '.join(k.build_args())} --no-run` failed ({how})"
        ctx.say(f"  {phase}: built {k.crate}" + (f" +{','.join(k.features)}" if k.features else "") + f" in {run.seconds:.0f} s")
    return None


def control_phase(ctx: Ctx, info: Info) -> Optional[Result]:
    """The killers on the tree as it is: green, and at least one test each. A Result when something is wrong."""
    row, rel = info.row, info.rel
    bad = build_all(ctx, info.header.killers, "control")
    if bad:
        return Result("INCONCLUSIVE", row, rel, f"the tree does not build without the patch: {bad}")
    for k in info.header.killers:
        run = cargo_run(ctx, f"control-{k.crate}-{k.filter}", k.run_args(), ctx.test_timeout)
        if run.rc is None:
            return Result("INCONCLUSIVE", row, rel, f"{k.spec()}: timed out after {ctx.test_timeout} s without the patch")
        if run.results == 0:
            ctx.say(tail(run.text))
            return Result("INCONCLUSIVE", row, rel, f"{k.spec()}: no test summary without the patch (exit {run.rc})")
        if run.ran == 0:
            return Result("NOKILLER", row, rel, f"{k.spec()} matches no test that runs (0 tests; ignored: {run.ignored})")
        if run.failed:
            return Result("CONTROL-RED", row, rel, f"{k.spec()} is RED without the patch ({run.first_failure}): a red test kills nothing")
        ctx.say(f"  control: {k.spec()}  {run.passed} test(s) green")
    return None


def mutant_phase(ctx: Ctx, info: Info) -> Result:
    row, rel = info.row, info.rel
    bad = build_all(ctx, info.header.killers, "patched")
    if bad:
        return Result("NOBUILD", row, rel, f"the patched tree does not build: {bad}")
    killed: List[str] = []
    survived: List[str] = []
    for k in info.header.killers:
        run = cargo_run(ctx, f"patched-{k.crate}-{k.filter}", k.run_args(), ctx.test_timeout)
        if run.rc is None:
            return Result("INCONCLUSIVE", row, rel, f"{k.spec()}: timed out after {ctx.test_timeout} s with the patch")
        if run.results == 0:
            ctx.say(tail(run.text))
            return Result("INCONCLUSIVE", row, rel, f"{k.spec()}: no test summary with the patch (crash? exit {run.rc})")
        if run.ran == 0:
            return Result("NOKILLER", row, rel, f"{k.spec()} matches no test with the patch applied")
        if run.failed:
            ctx.say(f"  patched: {k.spec()}  RED ({run.failed} of {run.ran}); first failure: {run.first_failure}")
            killed.append(f"{k.spec()} [{run.first_failure[:140]}]")
        else:
            ctx.say(f"  patched: {k.spec()}  still GREEN ({run.passed} test(s)): the guard is not held by it")
            survived.append(k.spec())
    if survived:
        return Result("SURVIVED", row, rel, "green with the patch applied: " + ", ".join(survived))
    return Result("KILLED", row, rel, f"{len(killed)} killer(s) red with the patch, green without: " + "; ".join(killed))


def replay(ctx: Ctx, path: Path, dry: bool) -> Result:
    t0 = time.monotonic()
    rel = rel_to_root(ctx.root, path)
    row = "-"
    try:
        info = load_patch(ctx, path)
        row = info.row
        head = head_applies(ctx, info)
        ctx.say(f"  applies to HEAD {head}; touches {', '.join(info.touched)}; killers: {', '.join(k.spec() for k in info.header.killers)}")
        if dry:
            res = Result("APPLIES", row, rel, f"applies to HEAD {head}; {len(info.touched)} file(s); {len(info.header.killers)} killer(s)")
        else:
            refuse_while_journal(ctx)
            require_clean(ctx, info)
            res = replay_patch(ctx, info)
    except Refusal as r:
        res = Result(r.verdict, row, rel, r.detail)
    res.seconds = time.monotonic() - t0
    return res


def markers(root: Path, paths: List[str]) -> Dict[str, int]:
    out: Dict[str, int] = {}
    for p in paths:
        with contextlib.suppress(OSError):
            out[p] = len(CONFLICT_FILE.findall((root / p).read_text(encoding="utf-8", errors="replace")))
    return out


def replay_patch(ctx: Ctx, info: Info) -> Result:
    early = control_phase(ctx, info)
    if early is not None:
        return early
    require_clean(ctx, info)  # again: the control run took minutes, and the tree is the other lanes' too
    pre = snapshot(ctx.root, info.touched)
    before = markers(ctx.root, info.touched)
    applied: Dict[str, int] = {}
    result: Optional[Result] = None
    journal = journal_open(ctx, info, pre)  # before the apply: a SIGKILL or a reboot from here on leaves `--recover` its facts
    try:
        done = git(ctx.root, "apply", str(info.path))
        if done.returncode != 0:
            result = Result("NOAPPLY", info.row, info.rel, "does not apply to the working tree: " + (done.stderr.strip().splitlines() or ["?"])[0])
        else:
            applied = mtimes(ctx.root, info.touched)
            ctx.say("  patch applied")
            after = markers(ctx.root, info.touched)
            bad = [p for p in info.touched if after.get(p, 0) > before.get(p, 0)]
            if bad:
                result = Result("NOAPPLY", info.row, info.rel, f"a conflict marker in {', '.join(bad)} after the apply")
            else:
                result = mutant_phase(ctx, info)
    finally:
        problems = restore(ctx, info, pre, applied)
        if problems:
            ctx.say("  RESTORE FAILED: " + "; ".join(problems) + " (the journal stays: `mutants.py --recover`)")
        else:
            with contextlib.suppress(OSError):
                journal.unlink()
            ctx.say("  patch reversed; proved: hashes equal, git status and git diff --stat empty, mtimes bumped")
    if problems:
        return Result("RESTORE-FAILED", info.row, info.rel, "; ".join(problems) + f" -- by hand: git apply -R {info.rel}")
    assert result is not None
    return result


# --- which patches -------------------------------------------------------------------------------------------------


def all_patches(ctx: Ctx) -> List[Path]:
    return sorted(ctx.patch_dir.glob("*.patch")) if ctx.patch_dir.is_dir() else []


def range_patches(ctx: Ctx, spec: str) -> List[Path]:
    """The patches of the folder added or changed between A and B (two refs joined by `..`)."""
    sides = spec.split("..")
    if len(sides) != 2 or "..." in spec or not all(RANGE_SIDE.match(s) and not s.startswith("-") for s in sides):
        raise Refusal("NOAPPLY", f"`{spec}` is not `<base>..<head>` (two refs, a plain range)")
    for s in sides:
        if git(ctx.root, "rev-parse", "--verify", "--quiet", s + "^{commit}").returncode != 0:
            raise Refusal("NOAPPLY", f"`{s}` is no commit of this repository (does the checkout hold the whole history?)")
    rel = rel_to_root(ctx.root, ctx.patch_dir)
    done = git(ctx.root, "diff", "--name-only", "--diff-filter=AM", "-z", sides[0], sides[1], "--", rel)
    out: List[Path] = []
    for name in done.stdout.split("\0"):
        if name.endswith(".patch") and "/" not in name[len(rel) + 1:]:
            if (ctx.root / name).is_file():
                out.append(ctx.root / name)
    return sorted(out)


def resolve_patch(ctx: Ctx, arg: str) -> Path:
    """A path as typed: absolute, relative to the working directory (the lanes work from iso/), or relative to the root."""
    p = Path(arg)
    if p.is_absolute():
        return p
    for base in (Path.cwd(), ctx.root):
        if (base / p).is_file():
            return base / p
    return Path.cwd() / p


def list_patches(ctx: Ctx) -> int:
    for p in all_patches(ctx):
        try:
            h = parse_header(p.read_text(encoding="utf-8"))
            ctx.say(f"{rel_to_root(ctx.root, p)}\t{','.join(h.rows) or '-'}\t{', '.join(k.spec() for k in h.killers) or '-'}")
        except (OSError, UnicodeDecodeError) as e:
            ctx.say(f"{rel_to_root(ctx.root, p)}\t-\tunreadable: {e}")
    return 0


# --- the entry --------------------------------------------------------------------------------------------------------


def github_report(results: List[Result]) -> None:
    if os.environ.get("GITHUB_ACTIONS") == "true":
        for r in results:
            if r.verdict not in ("KILLED", "APPLIES"):
                msg = re.sub(r"[\r\n%]+", " ", f"{r.verdict}: {r.detail}")
                print(f"::error file={r.patch}::{msg}")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary and results:
        rows = ["| verdict | row | patch | detail |", "|---|---|---|---|"]
        for r in results:
            rows.append(f"| {r.verdict} | {r.row} | `{r.patch}` | {r.detail[:300].replace('|', '/')} |")
        with contextlib.suppress(OSError), open(summary, "a", encoding="utf-8") as fh:
            fh.write("\n".join(rows) + "\n")


def run_all(ctx: Ctx, patches: List[Path], dry: bool, label: str) -> int:
    results: List[Result] = []
    for p in patches:
        ctx.say(f"mutants: {rel_to_root(ctx.root, p)}" + (" (dry run)" if dry else ""))
        res = replay(ctx, p, dry)
        ctx.say(f"  -> {res.verdict}: {res.detail}  ({res.seconds:.0f} s)")
        if res.verdict == "SURVIVED":
            ctx.say(f"  the row is NOT closed: the coordinator reopens it with  issues_status.py --set {res.row.split(',')[0]}=OPEN")
        results.append(res)
    return finish(ctx, results, label)


def finish(ctx: Ctx, results: List[Result], label: str) -> int:
    ctx.say(f"\n== RESULT (mutants.py {label})")
    if not results:
        ctx.say("no patch to replay: nothing was run (that is the answer for this range, not a pass of anything)")
    for r in results:
        ctx.say(r.line())
    counts: Dict[str, int] = {}
    for r in results:
        counts[r.verdict] = counts.get(r.verdict, 0) + 1
    ctx.say("patches: " + str(len(results)) + "".join(f", {n} {v}" for v, n in sorted(counts.items())))
    github_report(results)
    return max((r.code for r in results), default=0)


def build_ctx(args: argparse.Namespace) -> Ctx:
    here = Path(__file__).resolve().parent
    if args.root:
        root = Path(args.root).resolve()
    else:
        top = subprocess.run(["git", "-C", str(here), "rev-parse", "--show-toplevel"], capture_output=True, text=True)
        if top.returncode != 0:
            raise SystemExit("mutants.py: not inside a git repository")
        root = Path(top.stdout.strip()).resolve()
    iso = (root / "iso" / "Cargo.toml").is_file()
    patch_dir = Path(args.patch_dir) if args.patch_dir else (root / ("iso/mutants" if iso else "mutants"))
    workdir = Path(args.workdir) if args.workdir else (root / "iso" if iso else root)
    env = dict(os.environ)
    # the keychain is never the real one, whoever calls (the registry-mode runner did the same); no colours in a log
    for k, v in (("AZ_KEYRING", "memory"), ("AZLIN_KEYCHAIN", "file"), ("AZLIN_KEYCHAIN_PASSPHRASE", "test"),
                 ("CARGO_INCREMENTAL", "0"), ("CARGO_TERM_COLOR", "never"), ("NO_COLOR", "1"), ("RUST_BACKTRACE", "0")):
        env.setdefault(k, v)
    return Ctx(
        root=root,
        workdir=workdir.resolve(),
        patch_dir=patch_dir.resolve(),
        cargo=shlex.split(args.cargo),
        env=env,
        test_timeout=args.test_timeout,
        build_timeout=args.build_timeout or None,
        log_dir=Path(args.log_dir) if args.log_dir else None,
        many_ok=args.many or os.environ.get("GITHUB_ACTIONS") == "true",
        say=lambda s: print(s, flush=True),
    )


def main(argv: Optional[List[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--patch", action="append", metavar="FILE", help="replay this patch (repeatable)")
    mode.add_argument("--range", metavar="A..B", help="every patch added or changed in the range")
    mode.add_argument("--all", action="store_true", help="every patch of the folder")
    mode.add_argument("--ci", metavar="SPEC", help="`all` or `A..B`: the apply check of every patch, then the replays")
    mode.add_argument("--patches", action="store_true", help="list the patches, their rows and killers")
    mode.add_argument("--recover", action="store_true", help="reverse the patch an interrupted replay left applied (from its journal)")
    ap.add_argument("--dry-run", action="store_true", help="no build, no change: header, static checks, apply check against HEAD")
    ap.add_argument("--many", action="store_true", help="replay several patches outside GitHub Actions (never on the 8 GiB Mac)")
    ap.add_argument("--root", help="the repository (default: the one this script is in)")
    ap.add_argument("--patch-dir", help="the folder of the patches (default: iso/mutants, or mutants)")
    ap.add_argument("--workdir", help="where cargo runs (default: iso, or the root)")
    ap.add_argument("--cargo", default="cargo", help="the cargo command (a test seam; default: cargo)")
    ap.add_argument("--test-timeout", type=int, default=900, help="seconds for one killer run (default 900)")
    ap.add_argument("--build-timeout", type=int, default=0, help="seconds for one build (default: none)")
    ap.add_argument("--log-dir", help="keep the full output of every cargo call here")
    args = ap.parse_args(argv)
    ctx = build_ctx(args)
    try:
        with signals_caught():
            if args.patches:
                return list_patches(ctx)
            if args.recover:
                return recover(ctx)
            label = "--dry-run" if args.dry_run else "replay"
            if args.patch is not None:
                paths = [resolve_patch(ctx, p) for p in args.patch]
                return guard_many(ctx, paths, args.dry_run, lambda: run_all(ctx, paths, args.dry_run, label))
            try:
                if args.range is not None:
                    paths = range_patches(ctx, args.range)
                    return guard_many(ctx, paths, args.dry_run, lambda: run_all(ctx, paths, args.dry_run, f"--range {args.range}"))
                if args.all:
                    paths = all_patches(ctx)
                    return guard_many(ctx, paths, args.dry_run, lambda: run_all(ctx, paths, args.dry_run, "--all " + label))
                return run_ci(ctx, args.ci or "", args.dry_run)
            except Refusal as r:
                ctx.say(f"mutants: {r.verdict}: {r.detail}")
                return VERDICT_EXIT[r.verdict]
    except Interrupted as e:
        ctx.say(f"mutants: interrupted (signal {e.code - 128}); see the lines above for whether the patch was reversed and proved")
        return e.code
    except Exception:
        traceback.print_exc()
        return INTERNAL_ERROR


def guard_many(ctx: Ctx, paths: List[Path], dry: bool, go: Callable[[], int]) -> int:
    """Several replays are Linux CI's job: on the 8 GiB Mac one patch runs at a time inside one lock hold."""
    if len(paths) > 1 and not dry and not ctx.many_ok:
        ctx.say(
            f"mutants: refusing to replay {len(paths)} patches here: one patch at a time inside one build-lock hold "
            "(--patch FILE); Linux CI sets GITHUB_ACTIONS; --many overrides"
        )
        return VERDICT_EXIT["NOAPPLY"]
    return go()


def run_ci(ctx: Ctx, spec: str, dry: bool) -> int:
    """What the workflow runs. Every patch must still apply to HEAD (cheap, no build), then the replays of the selection."""
    spec = spec.strip()
    if spec != "all":
        selection = range_patches(ctx, spec)
    else:
        selection = all_patches(ctx)
    ctx.say(f"mutants: the apply check of every patch against HEAD ({len(all_patches(ctx))} patch(es))")
    codes = [run_all(ctx, all_patches(ctx), True, "--all --dry-run")]
    if not dry:
        codes.append(guard_many(ctx, selection, False, lambda: run_all(ctx, selection, False, f"--ci {spec}")))
    return max(codes)


if __name__ == "__main__":
    sys.exit(main())
