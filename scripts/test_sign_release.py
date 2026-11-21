#!/usr/bin/env python3
"""Tests of scripts/sign-release.sh with a FAKE signer (stdlib only; no minisign needed).

    python3 scripts/test_sign_release.py
    python3 -m unittest discover -s scripts -p 'test_sign_release.py'

The fake `minisign` writes recognisable key and signature files and logs every call, so a test can
say WHICH key signed WHAT without a real signing tool. The real chain is checked by the `y3_*`
tests of layout/src/updater.rs and by the script's own self-check, which these tests replace with
a recording stand-in (SIGN_RELEASE_VERIFY).

Register rows: AZ-Y12 (keys are minted on purpose, encrypted, outside git; the root key signs
statements only) and AZ-Y3 (the release is signed as a release statement, by the signing key).
"""

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
SCRIPT = os.path.join(HERE, "sign-release.sh")
CHECK = os.path.join(HERE, "check_no_test_hooks.py")

# A signer that makes recognisable files and logs what it was asked: a key is a KEYID line, a
# signature names the key that made it and the SHA-256 of what it covers.
FAKE_MINISIGN = r'''#!/usr/bin/env python3
import base64, hashlib, json, os, sys

args = sys.argv[1:]


def opt(name):
    return args[args.index(name) + 1] if name in args else None


record = {"argv": args}
if "-G" in args:
    pub, sec = opt("-p"), opt("-s")
    encrypted = "-W" not in args
    key_id = hashlib.sha256(os.path.realpath(sec).encode()).hexdigest()[:16]
    b64 = base64.b64encode(("RWQ-" + key_id).encode()).decode()
    with open(pub, "w") as f:
        f.write("untrusted comment: minisign public key FAKE\n" + b64 + "\n")
    with open(sec, "w") as f:
        f.write("untrusted comment: fake secret key\nKEYID=" + key_id + "\nENCRYPTED=" + ("1" if encrypted else "0") + "\n")
    record.update(kind="generate", sec=sec, pub=pub, encrypted=encrypted)
elif "-S" in args:
    sec, msg, sig = opt("-s"), opt("-m"), opt("-x")
    key_id = [l for l in open(sec).read().splitlines() if l.startswith("KEYID=")][0][len("KEYID="):]
    data = open(msg, "rb").read()
    digest = hashlib.sha256(data).hexdigest()
    with open(sig, "w") as f:
        f.write("untrusted comment: %s\nSIG key=%s over=%s\ntrusted comment: %s\nGLOBAL\n" % (opt("-c"), key_id, digest, opt("-t")))
    record.update(
        kind="sign",
        sec=sec,
        prehashed="-H" in args,
        digest=digest,
        message=data.decode("utf-8", "replace") if len(data) < 4096 else "<big>",
    )
else:
    sys.exit("fake minisign: unknown mode: " + " ".join(args))
with open(os.environ["FAKE_MINISIGN_LOG"], "a") as log:
    log.write(json.dumps(record) + "\n")
'''

# The self-check stand-in: records what it was asked and the manifest it was given.
FAKE_VERIFY = r'''#!/usr/bin/env python3
import json, os, sys

with open(os.environ["FAKE_VERIFY_LOG"], "w") as f:
    json.dump({"argv": sys.argv[1:], "manifest": open(sys.argv[1]).read()}, f)
sys.exit(int(os.environ.get("FAKE_VERIFY_EXIT", "0")))
'''


def sha256_hex(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


class Sandbox:
    """A folder with the fake tools first on PATH, a home of its own and a log of the signer's calls."""

    def __init__(self):
        self.root = os.path.realpath(tempfile.mkdtemp(prefix="sign-release-test-"))
        self.bin = os.path.join(self.root, "bin")
        self.home = os.path.join(self.root, "home")
        self.work = os.path.join(self.root, "work")
        self.keys = os.path.join(self.root, "keys")
        for folder in (self.bin, self.home, self.work):
            os.mkdir(folder)
        self.log_path = os.path.join(self.root, "calls.jsonl")
        self.verify_log = os.path.join(self.root, "verify.json")
        self._install("minisign", FAKE_MINISIGN)
        self._install("verify", FAKE_VERIFY)
        os.symlink(sys.executable, os.path.join(self.bin, "python3"))
        self.git = shutil.which("git")
        if self.git:
            os.symlink(self.git, os.path.join(self.bin, "git"))
        self.env = {
            "PATH": os.pathsep.join([self.bin, "/usr/bin", "/bin"]),
            "HOME": self.home,
            "LC_ALL": "C",
            "FAKE_MINISIGN_LOG": self.log_path,
            "FAKE_VERIFY_LOG": self.verify_log,
            "SIGN_RELEASE_SIGNER": "minisign",
            "SIGN_RELEASE_VERIFY": os.path.join(self.bin, "verify"),
        }

    def _install(self, name, text):
        path = os.path.join(self.bin, name)
        with open(path, "w") as f:
            f.write(text)
        os.chmod(path, 0o755)

    def close(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def run(self, *args, env=None):
        full = dict(self.env)
        full.update(env or {})
        return subprocess.run(
            ["bash", SCRIPT, *args],
            cwd=self.work,
            env=full,
            capture_output=True,
            text=True,
        )

    def calls(self):
        if not os.path.exists(self.log_path):
            return []
        return [json.loads(line) for line in read(self.log_path).splitlines() if line]

    def key_path(self, name):
        return os.path.join(self.keys, name)

    def key_id(self, secret_file):
        for line in read(self.key_path(secret_file)).splitlines():
            if line.startswith("KEYID="):
                return line[len("KEYID="):]
        raise AssertionError("no KEYID in " + secret_file)

    def pub_b64(self, public_file):
        return read(self.key_path(public_file)).splitlines()[1]


class SignReleaseScript(unittest.TestCase):
    def setUp(self):
        self.sb = Sandbox()
        self.addCleanup(self.sb.close)
        self.artifact_bytes = b"azwriter artifact bytes\n"
        self.artifact = os.path.join(self.sb.work, "azwriter.bin")
        with open(self.artifact, "wb") as f:
            f.write(self.artifact_bytes)
        self.out = os.path.join(self.sb.work, "manifest.json")

    # -- helpers ---------------------------------------------------------------------------------

    def keyed(self, generation=1):
        """The key ceremony, with the fake signer."""
        steps = (
            ["init-root"],
            ["init-signing"],
            ["statement", "--generation", str(generation)],
        )
        for step in steps:
            r = self.sb.run(*step, "--keys", self.sb.keys)
            self.assertEqual(r.returncode, 0, r.stderr)

    def release_args(self, **extra):
        args = [
            "release",
            "--keys", self.sb.keys,
            "--app", "azwriter",
            "--artifact", self.artifact,
            "--version", "2.0.0",
            "--url", "https://dl.example.com/azwriter.bin",
            "--platform", "macos-aarch64",
            "--out", self.out,
        ]
        for flag, value in extra.items():
            args += ["--" + flag.replace("_", "-"), value]
        return args

    # -- AZ-Y12: keys are minted on purpose ------------------------------------------------------

    def test_y12_without_a_command_nothing_is_minted(self):
        """The old script minted both key pairs, unencrypted, in ./release-keys on its first run
        of ANY invocation; now a run without a command (or with the old flags) mints nothing."""
        r = self.sb.run("--artifact", self.artifact, "--version", "2.0.0", "--keys", self.sb.keys)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("command", r.stderr)
        self.assertEqual(self.sb.calls(), [], "the signer must not have been asked anything")
        self.assertFalse(os.path.exists(self.sb.keys))
        self.assertFalse(os.path.exists(os.path.join(self.sb.work, "release-keys")))

    def test_y12_a_release_without_keys_says_which_command_makes_them_and_mints_none(self):
        r = self.sb.run(*self.release_args())
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("init-signing", r.stderr)
        self.assertEqual(self.sb.calls(), [])
        self.assertFalse(os.path.exists(self.out))

    def test_y12_the_root_key_is_encrypted_and_never_overwritten(self):
        r = self.sb.run("init-root", "--keys", self.sb.keys)
        self.assertEqual(r.returncode, 0, r.stderr)
        generated = [c for c in self.sb.calls() if c["kind"] == "generate"]
        self.assertEqual(len(generated), 1)
        self.assertTrue(generated[0]["encrypted"], "the root secret key must be passphrase-protected")
        self.assertIn("ENCRYPTED=1", read(self.sb.key_path("root.key")))
        self.assertEqual(os.stat(self.sb.key_path("root.key")).st_mode & 0o777, 0o600)
        self.assertEqual(os.stat(self.sb.keys).st_mode & 0o777, 0o700)
        before = read(self.sb.key_path("root.key"))
        again = self.sb.run("init-root", "--keys", self.sb.keys)
        self.assertNotEqual(again.returncode, 0)
        self.assertIn("refusing to overwrite", again.stderr)
        self.assertEqual(read(self.sb.key_path("root.key")), before)
        self.assertEqual(len([c for c in self.sb.calls() if c["kind"] == "generate"]), 1)

    def test_y12_the_signing_key_is_encrypted_unless_asked_not_to(self):
        r = self.sb.run("init-signing", "--keys", self.sb.keys)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(self.sb.calls()[-1]["encrypted"])
        self.assertFalse(os.path.exists(self.sb.key_path("signing.unencrypted")))

        other = os.path.join(self.sb.root, "ci-keys")
        r = self.sb.run("init-signing", "--keys", other, "--unencrypted")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(self.sb.calls()[-1]["encrypted"])
        self.assertTrue(os.path.exists(os.path.join(other, "signing.unencrypted")))

    def test_y12_a_keys_folder_git_does_not_ignore_is_refused(self):
        if not self.sb.git:
            self.skipTest("no git")
        repo = os.path.join(self.sb.root, "repo")
        os.mkdir(repo)
        subprocess.run([self.sb.git, "init", "-q", repo], check=True, env=self.sb.env)
        keys = os.path.join(repo, "keys")

        r = self.sb.run("init-root", "--keys", keys)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("git", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(keys, "root.key")))
        self.assertEqual(self.sb.calls(), [])

        with open(os.path.join(repo, ".gitignore"), "w") as f:
            f.write("keys/\n")
        r = self.sb.run("init-root", "--keys", keys)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(os.path.exists(os.path.join(keys, "root.key")))

    def test_y12_a_folder_whose_secret_key_files_are_ignored_by_name_is_fine(self):
        if not self.sb.git:
            self.skipTest("no git")
        repo = os.path.join(self.sb.root, "repo2")
        os.mkdir(repo)
        subprocess.run([self.sb.git, "init", "-q", repo], check=True, env=self.sb.env)
        with open(os.path.join(repo, ".gitignore"), "w") as f:
            f.write("root.key\nsigning.key\n")
        r = self.sb.run("init-root", "--keys", os.path.join(repo, "k"))
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_y12_the_root_key_signs_the_key_statement_and_nothing_else(self):
        self.keyed(generation=2)
        calls = self.sb.calls()
        self.assertEqual([c["kind"] for c in calls], ["generate", "generate", "sign"])
        sign = calls[2]
        self.assertEqual(os.path.realpath(sign["sec"]), self.sb.key_path("root.key"))
        self.assertTrue(sign["prehashed"], "-H: the client accepts only prehashed signatures")
        expected = re.compile(
            r"^azul-signing-key-v1\|pubkey=%s\|expires=\d+\|generation=2$"
            % re.escape(self.sb.pub_b64("signing.pub"))
        )
        self.assertRegex(sign["message"], expected)
        self.assertFalse(sign["message"].endswith("\n"), "signed as exact bytes: no trailing newline")
        self.assertEqual(
            read(self.sb.key_path("signing-key-statement.txt")),
            sign["message"],
            "the statement is signed as exact bytes: no trailing newline",
        )
        self.assertIn(
            "key=" + self.sb.key_id("root.key"),
            read(self.sb.key_path("signing-key-statement.txt.minisig")),
        )

    # -- AZ-Y12 / AZ-Y3: a release is signed by the signing key, as a release statement ----------

    def test_y12_a_release_is_signed_by_the_signing_key_alone(self):
        """The old script signed a fresh statement with the ROOT key on every run; a release now
        needs no root secret key at all (the build machine does not have it)."""
        self.keyed(generation=1)
        os.remove(self.sb.key_path("root.key"))
        before = len(self.sb.calls())

        r = self.sb.run(*self.release_args())
        self.assertEqual(r.returncode, 0, r.stderr)

        signed = self.sb.calls()[before:]
        self.assertEqual([c["kind"] for c in signed], ["sign", "sign"])
        for call in signed:
            self.assertEqual(os.path.realpath(call["sec"]), self.sb.key_path("signing.key"))
            self.assertTrue(call["prehashed"])
        self.assertEqual(signed[0]["digest"], sha256_hex(self.artifact_bytes))

        latest = json.loads(read(self.out))["latest"]
        signing_id = self.sb.key_id("signing.key")
        self.assertEqual(latest["version"], "2.0.0")
        self.assertEqual(latest["download_url"], "https://dl.example.com/azwriter.bin")
        self.assertEqual(latest["digest"], "sha256:" + sha256_hex(self.artifact_bytes))
        self.assertIn("key=" + signing_id, latest["signature"])
        self.assertIn("over=" + sha256_hex(self.artifact_bytes), latest["signature"])
        # The stored key statement goes into the manifest as it is, with the root's signature.
        self.assertEqual(
            latest["signing_key_statement"], read(self.sb.key_path("signing-key-statement.txt"))
        )
        self.assertEqual(
            latest["signing_key_statement_sig"],
            read(self.sb.key_path("signing-key-statement.txt.minisig")),
        )
        # The release statement is signed by the SIGNING key, over exactly its bytes.
        statement = latest["release_statement"]
        self.assertIn("key=" + signing_id, latest["release_statement_sig"])
        self.assertIn("over=" + sha256_hex(statement.encode()), latest["release_statement_sig"])

    def test_y3_the_release_statement_names_app_version_platform_and_hash(self):
        self.keyed(generation=2)
        r = self.sb.run(*self.release_args())
        self.assertEqual(r.returncode, 0, r.stderr)
        statement = json.loads(read(self.out))["latest"]["release_statement"]
        self.assertRegex(
            statement,
            r"^azul-release-v1\|app=azwriter\|version=2\.0\.0\|platform=macos-aarch64"
            r"\|sha256=%s\|released=\d+\|min_generation=2$" % sha256_hex(self.artifact_bytes),
        )
        self.assertFalse(statement.endswith("\n"), "signed as exact bytes: no trailing newline")

    def test_y3_min_generation_follows_the_key_statement_and_may_be_lower_but_not_higher(self):
        self.keyed(generation=2)
        r = self.sb.run(*self.release_args(min_generation="1"))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(
            json.loads(read(self.out))["latest"]["release_statement"].endswith("|min_generation=1")
        )

        os.remove(self.out)
        r = self.sb.run(*self.release_args(min_generation="3"))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("min-generation", r.stderr)
        self.assertFalse(os.path.exists(self.out), "no manifest for a release no client would accept")

    def test_y3_the_self_check_is_asked_about_this_app_and_platform_and_sees_the_published_bytes(self):
        self.keyed()
        r = self.sb.run(*self.release_args())
        self.assertEqual(r.returncode, 0, r.stderr)
        asked = json.load(open(self.sb.verify_log))
        argv = asked["argv"]
        self.assertEqual(argv[1], self.artifact)
        self.assertEqual(argv[2], self.sb.pub_b64("root.pub"))
        self.assertEqual(argv[3:], ["--app", "azwriter", "--platform", "macos-aarch64"])
        self.assertEqual(asked["manifest"], read(self.out), "what was verified is what is published")

    def test_a_release_the_self_check_rejects_leaves_no_manifest_and_no_assets(self):
        self.keyed()
        sidecars = os.path.join(self.sb.root, "assets")
        r = self.sb.run(*self.release_args(sidecars=sidecars), env={"FAKE_VERIFY_EXIT": "1"})
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("did NOT verify", r.stderr)
        self.assertFalse(os.path.exists(self.out))
        self.assertFalse(os.path.exists(sidecars))

    def test_the_files_of_a_github_release_are_named_for_the_updater(self):
        self.keyed()
        sidecars = os.path.join(self.sb.root, "assets")
        r = self.sb.run(*self.release_args(sidecars=sidecars))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(
            sorted(os.listdir(sidecars)),
            [
                "azwriter.bin.minisig",
                "azwriter.bin.release-statement.txt",
                "azwriter.bin.release-statement.txt.minisig",
                "signing-key-statement.txt",
                "signing-key-statement.txt.minisig",
            ],
        )
        latest = json.loads(read(self.out))["latest"]
        self.assertEqual(read(os.path.join(sidecars, "azwriter.bin.release-statement.txt")), latest["release_statement"])
        self.assertEqual(read(os.path.join(sidecars, "azwriter.bin.minisig")), latest["signature"])

    def test_a_release_needs_the_signed_key_statement_the_app_and_clean_labels(self):
        self.sb.run("init-root", "--keys", self.sb.keys)
        self.sb.run("init-signing", "--keys", self.sb.keys)
        r = self.sb.run(*self.release_args())
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("statement", r.stderr)

        self.sb.run("statement", "--generation", "1", "--keys", self.sb.keys)
        args = self.release_args()
        without_app = args[:3] + args[5:]
        r = self.sb.run(*without_app)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("--app", r.stderr)

        r = self.sb.run(*self.release_args(app="az|writer"))
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse(os.path.exists(self.out))

    # -- X-DELTA-AZDRIVE-KEYS-37: a test build is not signed ------------------------------------

    def marker(self):
        """The marker azul-appkit's `anchors::TEST_HOOKS_MARKER` puts in a test build, read from its source."""
        source = read(os.path.join(REPO, "examples", "azul-appkit", "src", "anchors.rs"))
        # rustfmt may wrap the line after the `=`: whitespace, newlines included, stands between it and the text
        m = re.search(r'pub const TEST_HOOKS_MARKER: &str =\s*"([^"]+)";', source)
        self.assertIsNotNone(m, "anchors.rs names the marker")
        return m.group(1)

    def test_x_delta_azdrive_keys_37_an_artifact_that_holds_the_test_hooks_marker_is_not_signed(self):
        self.keyed()
        with open(self.artifact, "wb") as f:
            f.write(b"\x7fELF....." + self.marker().encode() + b".....more bytes")
        signed_before = len(self.sb.calls())
        r = self.sb.run(*self.release_args())
        self.assertNotEqual(r.returncode, 0, r.stdout)
        self.assertIn("test build", r.stderr)
        self.assertFalse(os.path.exists(self.out), "no manifest")
        self.assertEqual(len(self.sb.calls()), signed_before, "nothing was signed")
        # The same release of an artifact without it goes through.
        with open(self.artifact, "wb") as f:
            f.write(self.artifact_bytes)
        r = self.sb.run(*self.release_args())
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_x_delta_azdrive_keys_37_the_check_script_finds_the_marker_both_ways(self):
        marker = self.marker().encode()
        shipped = os.path.join(self.sb.work, "shipped.bin")
        test_build = os.path.join(self.sb.work, "test.bin")
        cut = os.path.join(self.sb.work, "cut.bin")
        with open(shipped, "wb") as f:
            f.write(b"a shipped binary" * 1000)
        with open(test_build, "wb") as f:
            f.write(b"x" * 100 + marker + b"y" * 100)
        # A marker that straddles the end of a read is found as well (1 MiB chunks).
        with open(cut, "wb") as f:
            f.write(b"z" * ((1 << 20) - 7) + marker + b"z" * 50)

        def check(*args):
            return subprocess.run([sys.executable, CHECK, *args], capture_output=True, text=True)

        self.assertEqual(check(shipped).returncode, 0)
        self.assertEqual(check(test_build).returncode, 1)
        self.assertEqual(check(cut).returncode, 1)
        self.assertEqual(check("--expect-marker", test_build).returncode, 0)
        self.assertEqual(check("--expect-marker", shipped).returncode, 1, "the negative control can fail")
        self.assertEqual(check(shipped, test_build).returncode, 1, "one bad file is enough")
        self.assertEqual(check("/no/such/file").returncode, 1)
        self.assertEqual(check().returncode, 2)

    # -- X-DELTA-STRUCT3-5: ANY build marker is refused, not the test-hooks one only -------------------------
    #
    # `memory-keyring` (azul-storage's `azlin_keyring()` as a map of the process) is a dev-dependency feature of
    # AzDrive, AzMail and the bridge; resolver 2 leaves it out of a plain release build, and `cargo build --release
    # --all-targets` turns it on for the binaries that ship, which then lose every drive key at exit. It leaves the
    # same kind of marker as `test-hooks`, and both scripts refuse the shared prefix `AZLIN-BUILD-MARKER:`.

    def marker_in(self, relative_path, const):
        """The marker the `pub const <const>: &str` of a source file holds."""
        source = read(os.path.join(REPO, *relative_path.split("/")))
        m = re.search(r'pub const %s: &str =\s*"([^"]+)";' % const, source)
        self.assertIsNotNone(m, "%s names %s" % (relative_path, const))
        return m.group(1)

    def memory_keyring_marker(self):
        """The marker azul-storage's `azul_keyring::MEMORY_KEYRING_MARKER` puts in a build with `memory-keyring`."""
        return self.marker_in("examples/azul-storage/src/azul_keyring.rs", "MEMORY_KEYRING_MARKER")

    def check(self, *args):
        return subprocess.run([sys.executable, CHECK, *args], capture_output=True, text=True)

    def blob(self, name, *parts):
        """A file of the sandbox that holds `parts`, and its path."""
        path = os.path.join(self.sb.work, name)
        with open(path, "wb") as f:
            f.write(b"".join(parts))
        return path

    def test_x_delta_struct3_5_an_artifact_that_holds_the_memory_keyring_marker_is_not_signed(self):
        self.keyed()
        marker = self.memory_keyring_marker()
        self.assertTrue(marker.startswith("AZLIN-BUILD-MARKER:memory-keyring:"), marker)
        with open(self.artifact, "wb") as f:
            f.write(b"\x7fELF....." + marker.encode() + b".....more bytes")
        signed_before = len(self.sb.calls())
        r = self.sb.run(*self.release_args())
        self.assertNotEqual(r.returncode, 0, r.stdout)
        self.assertIn("test build", r.stderr)
        self.assertIn("memory-keyring", r.stderr, "the refusal names the feature")
        self.assertFalse(os.path.exists(self.out), "no manifest")
        self.assertEqual(len(self.sb.calls()), signed_before, "nothing was signed")
        # The same release of an artifact without any marker goes through (the negative control).
        with open(self.artifact, "wb") as f:
            f.write(self.artifact_bytes)
        r = self.sb.run(*self.release_args())
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_x_delta_struct3_5_the_test_hooks_marker_is_still_refused_and_so_is_one_nobody_has_written_yet(self):
        self.keyed()
        for marker in (
            self.marker().encode(),
            b"AZLIN-BUILD-MARKER:a-test-seam-of-the-future:why",
            b"AZLIN-BUILD-MARKER:",
        ):
            with open(self.artifact, "wb") as f:
                f.write(b"\x7fELF....." + marker + b".....more bytes")
            r = self.sb.run(*self.release_args())
            self.assertNotEqual(r.returncode, 0, marker)
            self.assertIn("test build", r.stderr)
            self.assertFalse(os.path.exists(self.out), "no manifest for %r" % marker)

    def test_x_delta_struct3_5_an_artifact_that_cannot_be_read_is_not_taken_for_one_without_a_marker(self):
        if os.geteuid() == 0:
            self.skipTest("root reads a file that is mode 000")
        self.keyed()
        os.chmod(self.artifact, 0)
        self.addCleanup(os.chmod, self.artifact, 0o644)
        r = self.sb.run(*self.release_args())
        self.assertNotEqual(r.returncode, 0, r.stdout)
        self.assertIn("cannot read", r.stderr)
        self.assertFalse(os.path.exists(self.out), "no manifest")
        self.assertEqual(self.check(self.artifact).returncode, 1, "the check script fails on it too")

    def test_x_delta_struct3_5_the_check_script_refuses_every_build_marker_and_names_it(self):
        prefix = b"AZLIN-BUILD-MARKER:"
        memory = self.memory_keyring_marker().encode()
        hooks = self.marker().encode()
        clean = self.blob("clean.bin", b"a shipped binary" * 1000)
        with_memory = self.blob("memory.bin", b"x" * 100, memory, b"y" * 100)
        with_hooks = self.blob("hooks.bin", b"x" * 100, hooks, b"y" * 100)
        with_both = self.blob("both.bin", b"x" * 100, hooks, b"\x00" * 9, memory, b"y" * 100)
        future = self.blob("future.bin", b"x" * 100, b"AZLIN-BUILD-MARKER:a-test-seam-of-the-future:why", b"y")
        bare = self.blob("bare.bin", b"x" * 10, prefix, b"\x00 and no readable name")
        # A marker that straddles the end of a read (1 MiB chunks): inside its prefix, and with the prefix at the
        # end of one chunk and its name in the next - the name is read whole all the same.
        cut_in_prefix = self.blob("cut-prefix.bin", b"z" * ((1 << 20) - 7), memory, b"z" * 50)
        cut_after_prefix = self.blob("cut-name.bin", b"z" * ((1 << 20) - len(prefix)), memory, b"z" * 50)

        self.assertEqual(self.check(clean).returncode, 0, "no marker, no refusal (the negative control)")
        for path in (with_memory, with_hooks, with_both, future, bare, cut_in_prefix, cut_after_prefix):
            r = self.check(path)
            self.assertEqual(r.returncode, 1, "%s: %s" % (path, r.stdout))
            self.assertIn("test build", r.stderr)
        self.assertIn("memory-keyring", self.check(with_memory).stderr)
        self.assertIn("test-hooks", self.check(with_hooks).stderr)
        both = self.check(with_both).stderr
        self.assertIn("memory-keyring", both)
        self.assertIn("test-hooks", both)
        self.assertIn("a-test-seam-of-the-future", self.check(future).stderr)
        self.assertIn("memory-keyring", self.check(cut_after_prefix).stderr, "a name cut by a read's end is read whole")
        self.assertEqual(self.check(clean, with_memory).returncode, 1, "one bad file is enough")
        self.assertEqual(self.check(with_memory, clean).returncode, 1, "and the order of the files does not matter")

    def test_x_delta_struct3_5_expect_marker_asks_for_a_marker_or_for_one_feature_and_can_fail(self):
        memory = self.memory_keyring_marker().encode()
        hooks = self.marker().encode()
        clean = self.blob("clean.bin", b"a shipped binary" * 1000)
        with_memory = self.blob("memory.bin", b"x" * 100, memory, b"y" * 100)
        with_hooks = self.blob("hooks.bin", b"x" * 100, hooks, b"y" * 100)
        # any marker: the CI step's negative control for a test build
        self.assertEqual(self.check("--expect-marker", with_memory).returncode, 0)
        self.assertEqual(self.check("--expect-marker", with_hooks).returncode, 0)
        self.assertEqual(self.check("--expect-marker", clean).returncode, 1, "the negative control can fail")
        # one feature: a build with another marker is not the one that was asked for
        self.assertEqual(self.check("--expect-marker=memory-keyring", with_memory).returncode, 0)
        self.assertEqual(self.check("--expect-marker=test-hooks", with_hooks).returncode, 0)
        self.assertEqual(self.check("--expect-marker=memory-keyring", with_hooks).returncode, 1)
        self.assertEqual(self.check("--expect-marker=test-hooks", with_memory).returncode, 1)
        self.assertEqual(self.check("--expect-marker=memory-keyring", clean).returncode, 1)
        self.assertEqual(self.check("--expect-marker=", with_memory).returncode, 2, "an empty name is no request")

    def test_x_delta_struct3_5_the_markers_of_the_source_are_the_ones_the_scripts_refuse(self):
        # Every marker the source can put in a binary has the shared prefix and the name of its feature: a marker
        # spelt another way would pass both scripts.
        for path, const, feature in (
            ("examples/azul-appkit/src/anchors.rs", "TEST_HOOKS_MARKER", "test-hooks"),
            ("core/src/run_mode.rs", "TEST_HOOKS_MARKER", "test-hooks"),
            ("examples/azul-storage/src/azul_keyring.rs", "MEMORY_KEYRING_MARKER", "memory-keyring"),
        ):
            marker = self.marker_in(path, const)
            self.assertTrue(marker.startswith("AZLIN-BUILD-MARKER:%s:" % feature), "%s %s: %s" % (path, const, marker))
            built = self.blob("source-marker.bin", b"\x7fELF", marker.encode(), b"\x00")
            self.assertEqual(self.check(built).returncode, 1, "%s %s is refused" % (path, const))
            self.assertEqual(self.check("--expect-marker=" + feature, built).returncode, 0, "%s %s" % (path, const))

    def test_help_prints_the_usage_and_exits_cleanly(self):
        r = self.sb.run("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("init-root", r.stdout)
        self.assertIn("release", r.stdout)


if __name__ == "__main__":
    unittest.main()
