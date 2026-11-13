#!/usr/bin/env python3
"""Tests of mutants.py (ARCH31-1) on small repositories made in a temporary directory, with a FAKE cargo that reads a
`fake_tests.json` of the repository and answers as libtest does: no build, no real cargo. What they plant, and what must be told:

  KILLED (the killer is red with the patch and green without; the tree is back, proved), SURVIVED (a killer that stays green
  with the patch), a patch that does not apply, a killer whose filter matches no test (exit 3, never a pass), a killer that is
  red without the patch (exit 3, and the tree is never touched), a patch that breaks the build, a crash and a timeout (neither
  is a kill), a file another lane modified (refused, its hunk untouched), a restore that cannot be proved, an interrupted replay
  (SIGTERM: restored; SIGKILL: the next run refuses and names the leftover), a conflict marker, a rename, a patch that creates
  a file, a header whose continuation line lost an indent (a killer that would be dropped), the journal and `--recover` after
  a SIGKILL, several patches outside CI, `--range`, `--all`, `--ci`, `--dry-run` against HEAD with a dirty working tree, the
  default features of azinit and azctl, and that the four real patches of iso/mutants still apply to HEAD.

    python3 -m unittest scripts/test_mutants.py        (from iso/; Python 3.9 or later, standard library only)
"""
import contextlib
import hashlib
import io
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, str(HERE))
import mutants  # noqa: E402

FAKE_CARGO = r'''
import json, os, signal, sys, time

args = sys.argv[1:]
log = os.environ.get("FAKE_CARGO_LOG")
if log:
    with open(log, "a") as fh:
        fh.write(" ".join(args) + "\n")
spec = json.load(open("fake_tests.json"))


def has(cond):
    path, text = cond
    try:
        return text in open(path).read()
    except OSError:
        return False


if "--no-run" in args:
    for t in spec["tests"]:
        if t.get("break_build_if") and has(t["break_build_if"]):
            print("error[E0425]: cannot find value `guard` in this scope\n --> src/lib.rs:1:1\nerror: could not compile `toy`")
            sys.exit(101)
    print("    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s")
    sys.exit(0)

rest = args[args.index("--") + 1:] if "--" in args else []
flt = rest[0] if rest else ""
exact = "--exact" in rest
chosen = [t for t in spec["tests"] if (t["name"] == flt if exact else flt in t["name"])]
runnable = [t for t in chosen if not t.get("ignored")]
ignored = len(chosen) - len(runnable)
print(f"\nrunning {len(runnable)} tests")
failed = []
for t in runnable:
    if t.get("kill_parent_if") and has(t["kill_parent_if"][:2]):
        os.kill(os.getppid(), getattr(signal, t["kill_parent_if"][2]))
        time.sleep(30)
    if t.get("edit_if") and has(t["edit_if"][:2]):
        open(t["edit_if"][2], "w").write(t["edit_if"][3])
    if t.get("hang_if") and has(t["hang_if"]):
        time.sleep(60)
    if t.get("crash_if") and has(t["crash_if"]):
        print(f"test {t['name']} ...", flush=True)
        os._exit(134)
    ok = has((t["file"], t["contains"])) if "contains" in t else not os.path.exists(t["absent"])
    print(f"test {t['name']} ... {'ok' if ok else 'FAILED'}")
    if not ok:
        failed.append(t)
if failed:
    print("\nfailures:\n")
    for t in failed:
        print(f"---- {t['name']} stdout ----")
        print(f"thread '{t['name']}' panicked at src/lib.rs:1:1:")
        print(f"assertion failed: {t.get('contains', 'absent ' + t.get('absent', ''))} in {t.get('file', '')}")
        print("note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n")
    print("failures:")
    for t in failed:
        print("    " + t["name"])
verdict = "FAILED" if failed else "ok"
print(f"\ntest result: {verdict}. {len(runnable) - len(failed)} passed; {len(failed)} failed; {ignored} ignored; 0 measured; "
      f"{len(spec['tests']) - len(chosen)} filtered out; finished in 0.00s\n")
sys.exit(101 if failed else 0)
'''

BASE_TESTS = {
    "tests": [
        {"name": "mod::guard_is_on", "file": "src/guard.txt", "contains": "guard=on"},
        {"name": "mod::other_is_one", "file": "src/other.txt", "contains": "other=1"},
    ]
}


def git_env():
    env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@t",
               GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1")
    for k in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"):
        env.pop(k, None)
    return env


class Fixture:
    """A repository of the azul shape (patches in mutants/, cargo at the root) with a fake cargo outside it."""

    def __init__(self, tests=None):
        self._dir = tempfile.TemporaryDirectory()
        base = Path(self._dir.name).resolve()
        self.root = base / "repo"
        self.root.mkdir()
        self.fake = base / "fakecargo.py"
        self.fake.write_text(FAKE_CARGO, encoding="utf-8")
        self.log = base / "cargo.log"
        self.git("init", "-q")
        self.write("src/guard.txt", "guard=on\n")
        self.write("src/other.txt", "other=1\n")
        self.write("fake_tests.json", json.dumps(tests or BASE_TESTS))
        self.commit("base")

    def close(self):
        self._dir.cleanup()

    def git(self, *args, check=True):
        done = subprocess.run(["git", "-C", str(self.root), "-c", "commit.gpgsign=false", *args], capture_output=True, text=True,
                              env=git_env())
        if check:
            assert done.returncode == 0, f"git {args}: {done.stderr}"
        return done.stdout

    def write(self, rel, text):
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")

    def read(self, rel):
        return (self.root / rel).read_text(encoding="utf-8")

    def commit(self, message="c"):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)
        return self.git("rev-parse", "HEAD").strip()

    def diff_of(self, edits, new_files=None):
        """The unified diff of the edits ({file: (old, new)}) and new files ({file: text}); the work tree is left as it was."""
        for rel, (old, new) in edits.items():
            self.write(rel, self.read(rel).replace(old, new))
        for rel, text in (new_files or {}).items():
            self.write(rel, text)
            self.git("add", "-N", rel)
        text = self.git("diff", "--", *edits, *(new_files or {}))
        for rel in edits:
            self.git("checkout", "--", rel)
        for rel in new_files or {}:
            self.git("rm", "-q", "--cached", rel)
            (self.root / rel).unlink()
        return text

    def patch(self, name, edits, row="G-1", killers="toy:mod::guard_is_on", new_files=None, header=None, commit=True):
        head = header if header is not None else f"# row: {row}\n# why: the guard line turned off\n# killers: {killers}\n"
        rel = f"mutants/{name}.patch"
        self.write(rel, head + self.diff_of(edits, new_files))
        if commit:
            self.commit(f"add {name}")
        return rel

    def set_tests(self, tests):
        self.write("fake_tests.json", json.dumps(tests))
        self.commit("tests")

    def calls(self):
        return self.log.read_text(encoding="utf-8").splitlines() if self.log.exists() else []

    def status(self):
        return self.git("status", "--porcelain")

    def run(self, *argv, env=None):
        buf = io.StringIO()
        base = {"FAKE_CARGO_LOG": str(self.log), "GITHUB_ACTIONS": "", "GITHUB_STEP_SUMMARY": ""}
        with mock.patch.dict(os.environ, {**base, **(env or {})}), contextlib.redirect_stdout(buf):
            code = mutants.main(["--root", str(self.root), "--cargo", f"{sys.executable} {self.fake}", *argv])
        return code, buf.getvalue()

    def results(self, out):
        return [ln.split("\t") for ln in out.splitlines() if ln.startswith("RESULT\t")]


GUARD_OFF = {"src/guard.txt": ("guard=on", "guard=off")}


class FixtureCase(unittest.TestCase):
    def setUp(self):
        self.fx = Fixture()
        self.addCleanup(self.fx.close)


class TheHeader(unittest.TestCase):
    def test_a_header_with_continuation_lines_and_features(self):
        h = mutants.parse_header(textwrap.dedent("""\
            # row: X-DELTA-A-1, X-DELTA-A-2
            # why: one line
            # killers: azinit:s3d::repair::tests::x_a, azlin-token:x_b
            #   azctl+dev:ctl::t::x_c
            #   toy+one+azlin-proto/other:x_d
            --- a/f
            +++ b/f
            """))
        self.assertEqual(h.problems, [])
        self.assertEqual(h.rows, ["X-DELTA-A-1", "X-DELTA-A-2"])
        specs = [(k.crate, k.features, k.filter, k.exact) for k in h.killers]
        self.assertEqual(specs, [
            ("azinit", ("dev",), "s3d::repair::tests::x_a", True),  # azinit gets dev by default; a full path is exact
            ("azlin-token", (), "x_b", False),
            ("azctl", ("dev",), "ctl::t::x_c", True),
            ("toy", ("one", "azlin-proto/other"), "x_d", False),
        ])
        self.assertEqual(h.killers[0].run_args(), ["test", "-p", "azinit", "--features", "dev", "--lib", "--", "s3d::repair::tests::x_a", "--exact"])
        self.assertEqual(h.killers[1].run_args(), ["test", "-p", "azlin-token", "--lib", "--", "x_b"])

    def test_a_malformed_header_says_every_problem(self):
        h = mutants.parse_header("# killers: azctl::ctl::x_legacy_double_colon, nocolon\n# why: w\n")
        self.assertTrue(any("no `# row:" in p for p in h.problems), h.problems)
        self.assertEqual(sum("is not `<crate>" in p for p in h.problems), 2, h.problems)
        self.assertTrue(mutants.parse_header("# row: A-1\n").problems)  # no killers

    def test_a_continuation_line_that_lost_its_indent_is_a_problem_not_a_dropped_killer(self):
        h = mutants.parse_header("# row: A-1\n# killers: toy:mod::a\n# toy:mod::b\n# why: w\n")
        self.assertTrue(any("`# toy:` is no header key" in p for p in h.problems), h.problems)
        self.assertEqual([k.filter for k in h.killers], ["mod::a"])
        ok = mutants.parse_header("# row: A-1\n# killers: toy:mod::a\n#   toy:mod::b\n# note: n\n# e.g.: free prose is no key\n# why: w\n")
        self.assertEqual((ok.problems, [k.filter for k in ok.killers]), ([], ["mod::a", "mod::b"]))

    def test_cargo_output_is_read_as_libtest_prints_it(self):
        failed = ("running 2 tests\ntest a::x ... ok\ntest a::y ... FAILED\n\nfailures:\n\n---- a::y stdout ----\n"
                  "thread 'a::y' panicked at src/x.rs:10:5:\nassertion `left == right` failed\n  left: 1\n right: 2\n"
                  "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\nfailures:\n    a::y\n\n"
                  "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s\n")
        r = mutants.parse_run(101, failed, 1.0)
        self.assertEqual((r.results, r.passed, r.failed, r.ran), (1, 1, 1, 2))
        self.assertIn("a::y: thread 'a::y' panicked at src/x.rs:10:5:", r.first_failure)
        self.assertIn("left: 1", r.first_failure)
        zero = mutants.parse_run(0, "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s\n", 0.1)
        self.assertEqual((zero.results, zero.ran), (1, 0))
        ign = mutants.parse_run(0, "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n", 0.1)
        self.assertEqual((ign.ran, ign.ignored), (0, 1))
        none = mutants.parse_run(134, "running 1 test\ntest a ...\n", 0.1)
        self.assertEqual(none.results, 0)  # a crash prints no summary: never a kill


class TheReplay(FixtureCase):
    def test_a_killer_red_with_the_patch_and_green_without_is_killed_and_the_tree_is_back(self):
        rel = self.fx.patch("G-1", GUARD_OFF)
        before = (self.fx.root / "src/guard.txt").stat().st_mtime_ns
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 0, out)
        (res,) = self.fx.results(out)
        self.assertEqual(res[1:4], ["KILLED", "G-1", rel])
        self.assertIn("panicked at src/lib.rs", res[4])  # the first failing assertion is in the summary line
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=on\n")
        self.assertEqual(self.fx.status(), "")
        self.assertGreater((self.fx.root / "src/guard.txt").stat().st_mtime_ns, before)  # bumped: cargo cannot reuse the mutant's build
        self.assertIn("patch reversed; proved", out)
        self.assertEqual(list((self.fx.root / ".git" / "mutants-journal").glob("*.json")), [])  # the journal is removed after the proof
        # the control ran on the untouched tree, then the patched build and run: 2 builds (--no-run) and 2 runs
        calls = self.fx.calls()
        self.assertEqual(sum("--no-run" in c for c in calls), 2, calls)
        self.assertEqual(sum("--no-run" not in c for c in calls), 2, calls)

    def test_a_killer_that_stays_green_with_the_patch_is_survived_and_the_row_is_not_closed(self):
        rel = self.fx.patch("G-2", GUARD_OFF, row="X-DELTA-G-2", killers="toy:mod::other_is_one")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 1, out)
        (res,) = self.fx.results(out)
        self.assertEqual(res[1], "SURVIVED")
        self.assertIn("toy:mod::other_is_one", res[4])
        self.assertIn("issues_status.py --set X-DELTA-G-2=OPEN", out)
        self.assertEqual(self.fx.status(), "")

    def test_a_patch_that_does_not_apply_is_exit_2_and_nothing_is_built(self):
        rel = self.fx.patch("G-3", GUARD_OFF)
        self.fx.write("src/guard.txt", "guard=on and more\n")  # HEAD moves on after the patch was made
        self.fx.commit("moved on")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 2, out)
        self.assertEqual(self.fx.results(out)[0][1], "NOAPPLY")
        self.assertIn("does not apply to HEAD", out)
        self.assertEqual(self.fx.calls(), [])
        code, out = self.fx.run("--patch", rel, "--dry-run")
        self.assertEqual((code, self.fx.results(out)[0][1]), (2, "NOAPPLY"))

    def test_a_killer_that_matches_no_test_is_exit_3_never_a_pass(self):
        rel = self.fx.patch("G-4", GUARD_OFF, killers="toy:mod::no_such_test")
        before = (self.fx.root / "src/guard.txt").stat().st_mtime_ns
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 3, out)
        self.assertEqual(self.fx.results(out)[0][1], "NOKILLER")
        self.assertIn("matches no test", out)
        self.assertEqual((self.fx.root / "src/guard.txt").stat().st_mtime_ns, before)  # decided on the control: the patch was never applied
        self.assertEqual(sum("--no-run" in c for c in self.fx.calls()), 1)
        self.fx.log.unlink()
        # a filter that only an ignored test matches runs nothing either
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [{"name": "mod::slow", "file": "src/guard.txt", "contains": "guard=on", "ignored": True}]})
        rel = self.fx.patch("G-4b", GUARD_OFF, killers="toy:mod::slow")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual((code, self.fx.results(out)[0][1]), (3, "NOKILLER"))

    def test_a_killer_red_without_the_patch_proves_nothing_and_the_tree_is_never_touched(self):
        self.fx.write("src/other.txt", "other=2\n")  # the killer other_is_one is red on the tree as it is
        self.fx.commit("other broke")
        rel = self.fx.patch("G-5", GUARD_OFF, killers="toy:mod::other_is_one")
        before = (self.fx.root / "src/guard.txt").stat().st_mtime_ns
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 3, out)
        self.assertEqual(self.fx.results(out)[0][1], "CONTROL-RED")
        self.assertEqual((self.fx.root / "src/guard.txt").stat().st_mtime_ns, before)  # not even a restore ran
        self.assertEqual(sum("--no-run" in c for c in self.fx.calls()), 1)

    def test_a_patch_that_breaks_the_build_is_no_kill(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [{"name": "mod::b", "file": "src/guard.txt", "contains": "x", "break_build_if": ["src/guard.txt", "BREAK"]}]})
        rel = self.fx.patch("G-6", {"src/guard.txt": ("guard=on", "BREAK")})
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 2, out)
        self.assertEqual(self.fx.results(out)[0][1], "NOBUILD")
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=on\n")
        self.assertEqual(self.fx.status(), "")

    def test_a_tree_that_does_not_build_without_the_patch_is_inconclusive_and_untouched(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [{"name": "mod::b", "file": "src/guard.txt", "contains": "x", "break_build_if": ["src/other.txt", "BREAK"]}]})
        rel = self.fx.patch("G-6b", GUARD_OFF)
        self.fx.write("src/other.txt", "BREAK\n")  # another lane's half-written file, in a file the patch does not touch
        before = (self.fx.root / "src/guard.txt").stat().st_mtime_ns
        code, out = self.fx.run("--patch", rel)
        self.assertEqual((code, self.fx.results(out)[0][1]), (4, "INCONCLUSIVE"), out)
        self.assertIn("the tree does not build without the patch", out)
        self.assertEqual((self.fx.root / "src/guard.txt").stat().st_mtime_ns, before)

    def test_a_git_status_that_fails_is_no_clean_tree(self):
        rel = self.fx.patch("G-6c", GUARD_OFF)
        (self.fx.root / ".git" / "index").write_bytes(b"not an index")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 2, out)
        self.assertIn("git status failed", out)
        self.assertEqual(self.fx.calls(), [])

    def test_a_crash_and_a_timeout_are_inconclusive_never_a_kill(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [
            {"name": "mod::crashes", "file": "src/guard.txt", "contains": "guard=on", "crash_if": ["src/guard.txt", "CRASH"]},
            {"name": "mod::hangs", "file": "src/guard.txt", "contains": "guard=on", "hang_if": ["src/guard.txt", "HANG"]},
        ]})
        crash = self.fx.patch("G-7", {"src/guard.txt": ("guard=on", "guard=on CRASH")}, killers="toy:mod::crashes")
        code, out = self.fx.run("--patch", crash)
        self.assertEqual((code, self.fx.results(out)[0][1]), (4, "INCONCLUSIVE"), out)
        self.assertIn("no test summary", out)
        hang = self.fx.patch("G-8", {"src/guard.txt": ("guard=on", "guard=on HANG")}, killers="toy:mod::hangs")
        code, out = self.fx.run("--patch", hang, "--test-timeout", "1")
        self.assertEqual((code, self.fx.results(out)[0][1]), (4, "INCONCLUSIVE"), out)
        self.assertIn("timed out", out)
        self.assertEqual(self.fx.status(), "")

    def test_a_file_another_lane_modified_is_refused_and_its_hunk_is_left_alone(self):
        rel = self.fx.patch("G-9", GUARD_OFF)
        self.fx.write("src/guard.txt", "guard=on\n# another lane's hunk\n")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 2, out)
        self.assertIn("refused, the working tree is not clean for src/guard.txt", out)
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=on\n# another lane's hunk\n")
        self.assertEqual(self.fx.calls(), [])

    def test_a_file_that_changes_under_the_replay_is_never_overwritten_and_the_restore_is_not_claimed(self):
        # another lane rewrites the mutated line while the patched build runs: `git apply -R` cannot find its hunk
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [
            {"name": "mod::edits", "file": "src/guard.txt", "contains": "guard=on", "edit_if": ["src/guard.txt", "guard=off", "src/guard.txt", "guard=other\n"]},
        ]})
        rel = self.fx.patch("G-10", GUARD_OFF, killers="toy:mod::edits")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 5, out)
        self.assertEqual(self.fx.results(out)[0][1], "RESTORE-FAILED")
        self.assertIn("git apply -R", out)
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=other\n")  # what the other lane wrote is still there

    def test_a_signal_reverses_the_patch_and_a_sigkill_leaves_a_journal_the_next_run_names_and_recover_undoes(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [
            {"name": "mod::term", "file": "src/guard.txt", "contains": "guard=on", "kill_parent_if": ["src/guard.txt", "guard=off", "SIGTERM"]},
            {"name": "mod::kill", "file": "src/guard.txt", "contains": "guard=on", "kill_parent_if": ["src/guard.txt", "guard=off", "SIGKILL"]},
        ]})
        term = self.fx.patch("G-11", GUARD_OFF, killers="toy:mod::term")
        kill = self.fx.patch("G-12", GUARD_OFF, killers="toy:mod::kill")
        env = dict(os.environ, FAKE_CARGO_LOG=str(self.fx.log), GITHUB_ACTIONS="")
        journal = self.fx.root / ".git" / "mutants-journal"

        def as_process(*argv):
            return subprocess.run([sys.executable, str(HERE / "mutants.py"), "--root", str(self.fx.root), "--cargo", f"{sys.executable} {self.fx.fake}", *argv],
                                  capture_output=True, text=True, env=env)

        done = as_process("--patch", term)  # SIGTERM: the handler unwinds through the finally, the tree is back, the journal is gone
        self.assertEqual(done.returncode, 128 + signal.SIGTERM, done.stdout + done.stderr)
        self.assertEqual(self.fx.status(), "")
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=on\n")
        self.assertIn("interrupted", done.stdout)
        self.assertEqual(list(journal.glob("*.json")), [])
        done = as_process("--patch", kill)  # SIGKILL: nothing can run; the patch stays applied, and the journal says so
        self.assertEqual(done.returncode, -signal.SIGKILL, done.stdout + done.stderr)
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=off\n")
        (entry,) = list(journal.glob("*.json"))
        facts = json.loads(entry.read_text(encoding="utf-8"))
        self.assertEqual((facts["patch"], facts["touched"]), (kill, ["src/guard.txt"]))
        self.assertEqual(facts["pre"]["src/guard.txt"][0], hashlib.sha256(b"guard=on\n").hexdigest())
        leftover_mtime = (self.fx.root / "src/guard.txt").stat().st_mtime_ns
        for other in (kill, term):  # no replay of ANY patch starts while the journal stands: its control run would build the mutant
            done = as_process("--patch", other)
            self.assertEqual(done.returncode, 2, done.stdout + done.stderr)
            self.assertIn(f"an earlier replay left its patch applied or is still running: {kill} (pid ", done.stdout)
            self.assertIn("mutants.py --recover", done.stdout)
        self.assertEqual(as_process("--patch", kill, "--dry-run").returncode, 0)  # the apply check against HEAD never needs the tree
        done = as_process("--recover")
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn(f"{kill}: recovered; proved", done.stdout)
        self.assertEqual(self.fx.status(), "")
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=on\n")
        self.assertGreater((self.fx.root / "src/guard.txt").stat().st_mtime_ns, leftover_mtime)
        self.assertEqual(list(journal.glob("*.json")), [])
        self.assertIn("nothing to recover", as_process("--recover").stdout)
        self.assertEqual(as_process("--patch", term).returncode, 128 + signal.SIGTERM)  # replays run again

    def test_without_a_journal_a_leftover_is_still_named_by_the_clean_tree_check(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [
            {"name": "mod::kill", "file": "src/guard.txt", "contains": "guard=on", "kill_parent_if": ["src/guard.txt", "guard=off", "SIGKILL"]},
        ]})
        kill = self.fx.patch("G-12b", GUARD_OFF, killers="toy:mod::kill")
        env = dict(os.environ, FAKE_CARGO_LOG=str(self.fx.log), GITHUB_ACTIONS="")
        argv = [sys.executable, str(HERE / "mutants.py"), "--root", str(self.fx.root), "--cargo", f"{sys.executable} {self.fx.fake}", "--patch", kill]
        self.assertEqual(subprocess.run(argv, capture_output=True, text=True, env=env).returncode, -signal.SIGKILL)
        for j in (self.fx.root / ".git" / "mutants-journal").glob("*.json"):
            j.unlink()  # the journal was lost (a copy of the checkout, a clean of .git)
        done = subprocess.run(argv, capture_output=True, text=True, env=env)
        self.assertEqual(done.returncode, 2, done.stdout + done.stderr)
        self.assertIn("not clean for src/guard.txt", done.stdout)
        self.assertIn(f"carry this patch (an interrupted replay): `git apply -R {kill}`", done.stdout)

    def test_recover_refuses_what_it_cannot_prove_and_a_running_replay_is_not_touched(self):
        rel = self.fx.patch("G-12c", GUARD_OFF)
        journal = self.fx.root / ".git" / "mutants-journal"
        journal.mkdir()
        pre = {"src/guard.txt": [hashlib.sha256(b"guard=on\n").hexdigest(), 0]}

        def entry(pid):
            (journal / "x.json").write_text(json.dumps({"patch": rel, "touched": ["src/guard.txt"], "pre": pre, "pid": pid, "started": "t"}), encoding="utf-8")

        entry(os.getppid())  # a replay that is still running (the parent of this test is alive)
        code, out = self.fx.run("--recover")
        self.assertEqual(code, 4, out)
        self.assertIn("still running", out)
        code, out = self.fx.run("--patch", rel)
        self.assertEqual((code, self.fx.results(out)[0][1]), (2, "NOAPPLY"), out)
        self.assertIn("still running", out)
        entry(2_000_000_000)  # a dead pid; another lane rewrote the file the patch had changed: the hunk cannot be reversed
        self.fx.write("src/guard.txt", "guard=off\nand a hunk of another lane\n")
        code, out = self.fx.run("--recover")
        self.assertEqual(code, 5, out)
        self.assertIn("NOT recovered", out)
        self.assertTrue((journal / "x.json").exists())  # the journal stays
        self.assertEqual(self.fx.read("src/guard.txt"), "guard=off\nand a hunk of another lane\n")  # and nothing was overwritten
        (journal / "x.json").write_text("not json", encoding="utf-8")
        code, out = self.fx.run("--recover")
        self.assertEqual(code, 5, out)
        self.assertIn("unreadable", out)

    def test_the_journal_is_written_before_the_patch_is_applied(self):
        rel = self.fx.patch("G-12e", GUARD_OFF)
        seen = []
        real = mutants.journal_open

        def spy(ctx, info, pre):
            seen.append(self.fx.read("src/guard.txt"))  # what the file holds at the moment the journal is written
            return real(ctx, info, pre)

        with mock.patch.object(mutants, "journal_open", spy):
            code, out = self.fx.run("--patch", rel)
        self.assertEqual((code, seen), (0, ["guard=on\n"]), out)

    def test_a_replay_that_fails_to_restore_keeps_its_journal(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [
            {"name": "mod::edits", "file": "src/guard.txt", "contains": "guard=on", "edit_if": ["src/guard.txt", "guard=off", "src/guard.txt", "guard=other\n"]},
        ]})
        rel = self.fx.patch("G-12d", GUARD_OFF, killers="toy:mod::edits")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 5, out)
        self.assertEqual(len(list((self.fx.root / ".git" / "mutants-journal").glob("*.json"))), 1)
        self.assertIn("mutants.py --recover", out)

    def test_a_patch_that_creates_a_file_is_reversed_by_removing_it(self):
        self.fx.set_tests({"tests": BASE_TESTS["tests"] + [{"name": "mod::no_extra", "absent": "src/extra.txt"}]})
        rel = self.fx.patch("G-13", {}, new_files={"src/extra.txt": "guard=off\n"}, killers="toy:mod::no_extra")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 0, out)
        self.assertFalse((self.fx.root / "src/extra.txt").exists())
        self.assertEqual(self.fx.status(), "")

    def test_features_and_exactness_reach_cargo(self):
        rel = self.fx.patch("G-14", GUARD_OFF, killers="azinit:mod::guard_is_on, azlin-token+azlin-proto/other:guard_is_on, toy:guard_is_on")
        code, out = self.fx.run("--patch", rel)
        self.assertEqual(code, 0, out)
        runs = [c for c in self.fx.calls() if "--no-run" not in c][:3]
        self.assertEqual(runs[0], "test -p azinit --features dev --lib -- mod::guard_is_on --exact")
        self.assertEqual(runs[1], "test -p azlin-token --features azlin-proto/other --lib -- guard_is_on")
        self.assertEqual(runs[2], "test -p toy --lib -- guard_is_on")


class TheStaticChecks(FixtureCase):
    def test_a_conflict_marker_a_rename_and_a_bad_header_are_refused_before_anything_runs(self):
        marker = self.fx.patch("C-1", {"src/guard.txt": ("guard=on", "<<<<<<< ours")})
        code, out = self.fx.run("--patch", marker, "--dry-run")
        self.assertEqual(code, 2, out)
        self.assertIn("adds a conflict marker", out)
        self.fx.git("mv", "src/other.txt", "src/moved.txt")
        text = self.fx.git("diff", "--cached", "-M")
        self.fx.git("reset", "-q", "--hard")
        self.fx.write("mutants/C-2.patch", "# row: C-2\n# killers: toy:x\n" + text)
        code, out = self.fx.run("--patch", "mutants/C-2.patch", "--dry-run")
        self.assertEqual(code, 2, out)
        self.assertIn("no rename, copy, mode or binary change", out)
        self.fx.write("mutants/C-3.patch", "# why: no row, no killers\n" + self.fx.diff_of(GUARD_OFF))
        code, out = self.fx.run("--patch", "mutants/C-3.patch", "--dry-run")
        self.assertEqual(code, 2, out)
        self.assertIn("header: no `# row:", out)
        self.assertEqual(self.fx.calls(), [])

    def test_a_dry_run_checks_against_head_whatever_the_working_tree_and_the_index_hold(self):
        rel = self.fx.patch("D-1", GUARD_OFF)
        self.fx.write("src/guard.txt", "something else entirely\n")  # a lane's uncommitted rewrite
        index_before = (self.fx.root / ".git/index").read_bytes()
        code, out = self.fx.run("--patch", rel, "--dry-run")
        self.assertEqual(code, 0, out)
        res = self.fx.results(out)[0]
        self.assertEqual(res[1], "APPLIES")
        self.assertIn("(dry run)", out)
        self.assertEqual((self.fx.root / ".git/index").read_bytes(), index_before)  # the real index was not touched
        self.assertEqual(self.fx.read("src/guard.txt"), "something else entirely\n")
        self.assertEqual(self.fx.calls(), [])


class TheSelection(FixtureCase):
    def test_range_replays_the_patches_added_or_changed_in_it_and_only_those(self):
        a = self.fx.patch("R-A", GUARD_OFF, row="R-A")
        c1 = self.fx.git("rev-parse", "HEAD").strip()
        b = self.fx.patch("R-B", {"src/other.txt": ("other=1", "other=0")}, row="R-B", killers="toy:mod::other_is_one")
        self.fx.write(a, self.fx.read(a).replace("the guard line", "the guard line, reworded"))
        c3 = self.fx.commit("reword A")
        code, out = self.fx.run("--range", f"{c1}..{c3}", "--many")
        self.assertEqual(code, 0, out)
        self.assertEqual(sorted(r[2] for r in self.fx.results(out)), ["R-A", "R-B"])  # A changed, B added
        code, out = self.fx.run("--range", f"{c3}..{c3}", "--many")
        self.assertEqual(code, 0, out)
        self.assertEqual(self.fx.results(out), [])
        self.assertIn("no patch to replay", out)
        code, out = self.fx.run("--range", "nonsense", "--many")
        self.assertEqual(code, 2, out)
        code, out = self.fx.run("--range=--output=x..HEAD", "--many")
        self.assertEqual(code, 2, out)

    def test_a_patch_renamed_in_the_range_is_replayed_under_its_new_name(self):
        a = self.fx.patch("N-A", GUARD_OFF, row="N-A")
        c1 = self.fx.git("rev-parse", "HEAD").strip()
        self.fx.git("mv", a, "mutants/N-A2.patch")
        c2 = self.fx.commit("rename the patch")
        code, out = self.fx.run("--range", f"{c1}..{c2}", "--many")
        self.assertEqual(code, 0, out)
        self.assertEqual([r[3] for r in self.fx.results(out)], ["mutants/N-A2.patch"])

    def test_a_second_signal_waits_until_the_restore_is_done(self):
        seen = []
        with mutants.signals_caught():
            try:
                with mutants.signals_blocked():
                    os.kill(os.getpid(), signal.SIGTERM)  # held: the handler has not run
                    seen.append("inside")
                seen.append("not reached")
            except mutants.Interrupted as e:
                seen.append(("interrupted", e.code))  # delivered when the block is left
        self.assertEqual(seen, ["inside", ("interrupted", 128 + signal.SIGTERM)])

    def test_the_restore_of_a_replay_runs_with_the_signals_blocked(self):
        rel = self.fx.patch("G-16", GUARD_OFF)
        seen = []
        real = mutants.restore

        def spy(ctx, info, pre, applied):
            seen.append(signal.SIGTERM in signal.pthread_sigmask(signal.SIG_BLOCK, []))
            return real(ctx, info, pre, applied)

        with mock.patch.object(mutants, "restore", spy):
            code, out = self.fx.run("--patch", rel)
        self.assertEqual((code, seen), (0, [True]), out)

    def test_several_replays_are_refused_outside_ci_and_run_inside_it(self):
        self.fx.patch("M-1", GUARD_OFF, row="M-1")
        self.fx.patch("M-2", GUARD_OFF, row="M-2")
        code, out = self.fx.run("--all")
        self.assertEqual(code, 2, out)
        self.assertIn("refusing to replay 2 patches here", out)
        self.assertEqual(self.fx.calls(), [])
        code, out = self.fx.run("--all", env={"GITHUB_ACTIONS": "true"})
        self.assertEqual(code, 0, out)
        self.assertEqual(sorted(r[2] for r in self.fx.results(out)), ["M-1", "M-2"])
        code, out = self.fx.run("--all", "--dry-run")  # the apply check is cheap: always allowed
        self.assertEqual(code, 0, out)

    def test_ci_checks_that_every_patch_still_applies_then_replays_the_selection(self):
        good = self.fx.patch("I-1", GUARD_OFF, row="I-1")
        c1 = self.fx.git("rev-parse", "HEAD").strip()
        stale = self.fx.patch("I-2", {"src/other.txt": ("other=1", "other=7")}, row="I-2", killers="toy:mod::other_is_one")
        self.fx.write("src/other.txt", "other=1 and more\n")
        c3 = self.fx.commit("moved on under I-2")
        code, out = self.fx.run("--ci", f"{c1}..{c3}", env={"GITHUB_ACTIONS": "true"})
        res = {r[2]: r[1] for r in self.fx.results(out)}
        self.assertEqual(res["I-2"], "NOAPPLY")  # bit-rot is found by the apply check of ALL patches...
        self.assertEqual(res["I-1"], "APPLIES")  # ...which comes first
        self.assertEqual(code, 2, out)
        code, out = self.fx.run("--ci", "all", "--dry-run")
        self.assertEqual(code, 2, out)
        self.assertEqual(self.fx.run("--ci", "")[0], 2)
        self.assertEqual(good, "mutants/I-1.patch")

    def test_the_summary_and_the_annotations_of_github(self):
        rel = self.fx.patch("S-1", GUARD_OFF, row="S-1", killers="toy:mod::other_is_one")
        summary = Path(self.fx.root).parent / "summary.md"
        code, out = self.fx.run("--patch", rel, env={"GITHUB_ACTIONS": "true", "GITHUB_STEP_SUMMARY": str(summary)})
        self.assertEqual(code, 1, out)
        self.assertIn(f"::error file={rel}::SURVIVED:", out)
        self.assertIn("| SURVIVED | S-1 |", summary.read_text(encoding="utf-8"))

    def test_patches_lists_the_rows_and_killers(self):
        self.fx.patch("L-1", GUARD_OFF, row="L-1", killers="toy:mod::guard_is_on")
        code, out = self.fx.run("--patches")
        self.assertEqual(code, 0)
        self.assertIn("mutants/L-1.patch\tL-1\ttoy:mod::guard_is_on", out)


class TheRealPatches(unittest.TestCase):
    def test_every_patch_of_this_repository_has_a_header_and_still_applies_to_head(self):
        """The nightly check of the brief, in the form every run of this file gives it: a patch that stopped applying (a refactor
        moved its lines) fails here and in the workflow's apply check, and the author rewrites it."""
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = mutants.main(["--all", "--dry-run"])
        out = buf.getvalue()
        self.assertEqual(code, 0, out)
        results = [ln.split("\t") for ln in out.splitlines() if ln.startswith("RESULT\t")]
        self.assertTrue(all(r[1] == "APPLIES" for r in results), out)
        names = {r[3].rsplit("/", 1)[-1] for r in results}
        top = subprocess.run(["git", "-C", str(HERE), "rev-parse", "--show-toplevel"], capture_output=True, text=True).stdout.strip()
        if top and (Path(top) / "iso" / "Cargo.toml").is_file():  # azul-apps (azul has no iso/): the four patches of OPS-CMD-9 and X-AR3-ND1
            self.assertTrue({"claim_server_refusal_deleted.patch", "page_unknown_class_resolves.patch"} <= names, names)


if __name__ == "__main__":
    unittest.main()
