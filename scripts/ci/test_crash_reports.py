#!/usr/bin/env python3
"""Tests of scripts/ci/crash_reports.py: whose report is it (python3 -I scripts/ci/test_crash_reports.py)."""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import crash_reports as c  # noqa: E402
import smoke_verdict  # noqa: E402

# the report that failed the boot test of AzMail, alive twice, on the screen and drawn (run 46118639960, 2026-11-28): the file name as the runner wrote it
EXACT = "STExtractionService_2026-11-28-073050_sjc20-bb660-5c7bb040-b3a6-467e-b17b-9319b7a1c21e-C6D928107DAB.diag"
DAEMON_DIAG = """\
Date/Time:        2026-11-28 07:26:46.089 +0000
End time:         2026-11-28 07:30:20.970 +0000
OS Version:       macOS 15.8.2 (Build 24H232)
Architecture:     arm64e
Report Version:   60
Incident Identifier: EE9D47DD-F7F8-4B31-B3B5-9CD3045E33D6

Data Source:      Microstackshots

Command:          STExtractionService
Path:             /Library/Developer/CoreSimulator/Volumes/iOS_23C54/Library/Developer/CoreSimulator/Profiles/Runtimes/iOS 26.2.simruntime/Contents/Resources/RuntimeRoot/System/Library/PrivateFrameworks/StreamingExtractor.framework/XPCServices/STExtractionService.xpc/STExtractionService
Identifier:       com.apple.STExtractionService
Version:          1.0 (1)
Resource Coalition: "com.apple.CoreSimulator.SimDevice.7B7DE7A1-7748-4EE4-8B6B-479E031064B3"(2496)
On Behalf Of:     58 samples UNKNOWN [64403] (57 samples originated by UNKNOWN [64332, 64471], 1 sample originated by modelcatalogd [64466])
Parent:           launchd_sim [64324]
PID:              66147

Event:            disk writes
Action taken:     none
"""
APP_IPS = (
    '{"app_name":"AzMail","timestamp":"2026-11-28 07:31:02.00 +0000","app_version":"0.1","build_version":"1","platform":2,'
    '"bundleID":"com.azul.AzMail","bug_type":"309","os_version":"iPhone OS 26.2 (23C54)","name":"AzMail","incident_id":"A1"}\n'
    '{\n  "procRole" : "Foreground",\n  "procName" : "AzMail",\n'
    '  "procPath" : "/Users/runner/Library/Developer/CoreSimulator/Devices/7B7D/data/Containers/Bundle/Application/1234/AzMail.app/AzMail",\n'
    '  "bundleInfo" : {"CFBundleShortVersionString":"0.1","CFBundleVersion":"1","CFBundleIdentifier":"com.azul.AzMail"}\n}\n'
)
# a system process whose report mentions the app as a thread and as an image: not the app's
SPRINGBOARD_IPS = (
    '{"app_name":"SpringBoard","bundleID":"com.apple.springboard","bug_type":"309","name":"SpringBoard","incident_id":"B2"}\n'
    '{\n  "procName" : "SpringBoard",\n  "procPath" : "/System/Library/CoreServices/SpringBoard.app/SpringBoard",\n'
    '  "bundleInfo" : {"CFBundleIdentifier":"com.apple.springboard"},\n'
    '  "threads" : [{"name":"AzMail"}], "usedImages" : [{"name":"AzMail"}]\n}\n'
)
EXE, BID = "AzMail", "com.azul.AzMail"


class Folder:
    """A temporary folder standing for ~/Library/Logs/DiagnosticReports, a marker older than the reports and a folder for the copies."""

    def __init__(self, test):
        self.tmp = tempfile.TemporaryDirectory()
        test.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.reports, self.out = root / "DiagnosticReports", root / "out"
        self.reports.mkdir()
        self.marker = root / "marker"
        self.marker.write_text("")
        os.utime(self.marker, (1000, 1000))

    def add(self, name, text="x", when=2000, sub=None):
        folder = self.reports / sub if sub else self.reports
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / name
        path.write_text(text)
        os.utime(path, (when, when))
        return path

    def classify(self, exe=EXE, bid=BID):
        mine, other = c.classify([self.reports], self.marker, exe, bid)
        return [p.name for p in mine], [p.name for p in other]


class WhoseReportTests(unittest.TestCase):
    def test_the_report_that_failed_the_boot_test_of_azmail_is_a_system_report(self):
        f = Folder(self)
        f.add(EXACT, DAEMON_DIAG)
        self.assertEqual(f.classify(), ([], [EXACT]))

    def test_the_same_file_name_without_a_body_is_still_not_the_apps(self):
        f = Folder(self)
        f.add(EXACT, "")
        self.assertEqual(f.classify(), ([], [EXACT]))

    def test_the_process_is_what_comes_before_the_date_stamp(self):
        self.assertEqual(c.process_name(EXACT), "STExtractionService")
        self.assertEqual(c.process_name("AzMail-2026-11-28-073050.ips"), "AzMail")
        self.assertEqual(c.process_name("AzMail_2026-11-28-073050_host.crash"), "AzMail")
        self.assertEqual(c.process_name("AzMail Helper-2026-11-28-073050.ips"), "AzMail Helper")
        self.assertEqual(c.process_name("AzMail-iOS-2026-11-28-073050.ips"), "AzMail-iOS")
        self.assertEqual(c.process_name("AzMail.ips"), "AzMail")

    def test_a_report_still_being_written_names_the_same_process(self):
        # the host writes `.<name>` first and renames it; the one a runner showed while the boot test ran (run 46123227484) is a daemon's
        real = ".STExtractionService_2026-11-28-075645_iad20-gt1022-f6a20b73-b5b3-4b56-94c9-feede2d719b6-764BEE249A43.diag"
        self.assertEqual(c.process_name(real), "STExtractionService")
        self.assertEqual(c.process_name(".AzMail-2026-11-28-073050.ips"), "AzMail")
        f = Folder(self)
        f.add(real, DAEMON_DIAG)
        f.add(".AzMail-2026-11-28-073102.ips")
        self.assertEqual(f.classify(), ([".AzMail-2026-11-28-073102.ips"], [real]))

    def test_every_kind_of_report_of_the_apps_own_process_is_the_apps(self):
        f = Folder(self)
        names = ["AzMail-2026-11-28-073050.ips", "AzMail_2026-11-28-073050_sjc20-host.crash", "AzMail_2026-11-28-073050_host.diag",
                 "AzMail_2026-11-28-073050_host.cpu_resource.diag", "AzMail_2026-11-28-073050_host.hang"]
        for n in names:
            f.add(n)
        mine, other = f.classify()
        self.assertEqual(sorted(mine), sorted(names))
        self.assertEqual(other, [])

    def test_a_helper_process_of_the_app_is_the_apps_but_another_process_with_its_prefix_is_not(self):
        f = Folder(self)
        f.add("AzMail Helper-2026-11-28-073050.ips")
        f.add("AzMail-ShareExtension-2026-11-28-073050.ips")
        f.add("AzMailer-2026-11-28-073050.ips")
        f.add("Mail-2026-11-28-073050.ips")
        mine, other = f.classify()
        self.assertEqual(sorted(mine), ["AzMail Helper-2026-11-28-073050.ips", "AzMail-ShareExtension-2026-11-28-073050.ips"])
        self.assertEqual(sorted(other), ["AzMailer-2026-11-28-073050.ips", "Mail-2026-11-28-073050.ips"])

    def test_an_empty_executable_names_nothing(self):
        f = Folder(self)
        f.add("AzMail-2026-11-28-073050.ips", APP_IPS)
        f.add(EXACT, DAEMON_DIAG)
        self.assertEqual(f.classify(exe="", bid=""), ([], ["AzMail-2026-11-28-073050.ips", EXACT]))

    def test_the_header_of_an_ips_names_the_app_when_the_file_name_does_not(self):
        f = Folder(self)
        f.add("Renamed-2026-11-28-073100.ips", APP_IPS)
        self.assertEqual(f.classify(), (["Renamed-2026-11-28-073100.ips"], []))

    def test_a_text_report_names_the_app_by_its_process_its_path_or_its_identifier(self):
        f = Folder(self)
        f.add("a-2026-11-28-073100.crash", "Process:         AzMail [66813]\nPath:  /x/y\n")
        f.add("b-2026-11-28-073100.diag", "Command:          Other\nPath:             /x/Containers/Bundle/Application/U/AzMail.app/AzMail\n")
        f.add("c-2026-11-28-073100.diag", "Command:          Other\nIdentifier:       com.azul.azmail\n")
        mine, other = f.classify()
        self.assertEqual(sorted(mine), ["a-2026-11-28-073100.crash", "b-2026-11-28-073100.diag", "c-2026-11-28-073100.diag"])
        self.assertEqual(other, [])

    def test_a_system_process_that_only_mentions_the_app_is_not_the_apps(self):
        f = Folder(self)
        f.add("SpringBoard-2026-11-28-073100.ips", SPRINGBOARD_IPS)               # the app as a thread name and an image name
        f.add("STExtractionService_2026-11-28-073101_host.diag", DAEMON_DIAG.replace("UNKNOWN [64403]", "AzMail [64403]"))   # "On Behalf Of"
        mine, other = f.classify()
        self.assertEqual(mine, [])
        self.assertEqual(sorted(other), ["STExtractionService_2026-11-28-073101_host.diag", "SpringBoard-2026-11-28-073100.ips"])

    def test_a_dump_of_several_processes_is_about_the_first_one(self):
        f = Folder(self)
        text = "Process:  SpringBoard [50]\nIdentifier: com.apple.springboard\n\nProcess:  AzMail [66813]\nIdentifier: com.azul.AzMail\n"
        f.add("hang-2026-11-28-073100.spin", text)
        self.assertEqual(f.classify(), ([], ["hang-2026-11-28-073100.spin"]))

    def test_both_kinds_side_by_side_the_apps_report_is_found_and_the_daemons_is_named_apart(self):
        f = Folder(self)
        f.add(EXACT, DAEMON_DIAG)
        f.add("AzMail-2026-11-28-073102.ips", APP_IPS)
        self.assertEqual(f.classify(), (["AzMail-2026-11-28-073102.ips"], [EXACT]))


class WhichFilesCountTests(unittest.TestCase):
    def test_only_files_newer_than_the_marker_count_and_the_retired_folder_is_looked_into(self):
        f = Folder(self)
        f.add("AzMail-2026-11-27-120000.ips", when=500)                         # older than the marker
        f.add("AzMail-2026-11-28-120000.ips", when=1000)                        # as old as the marker: `find -newer` says no
        f.add("AzMail-2026-11-28-120001.ips", when=1001)
        f.add("Other-2026-11-28-120002.ips", when=3000, sub="Retired")
        self.assertEqual(f.classify(), (["AzMail-2026-11-28-120001.ips"], ["Other-2026-11-28-120002.ips"]))

    def test_a_folder_that_is_not_there_is_nothing(self):
        f = Folder(self)
        mine, other = c.classify([f.reports / "nothing"], f.marker, EXE, BID)
        self.assertEqual((mine, other), ([], []))


class CollectTests(unittest.TestCase):
    def test_the_apps_reports_go_to_crash_and_the_others_to_crash_system_as_copies(self):
        f = Folder(self)
        a = f.add("AzMail-2026-11-28-073102.ips", APP_IPS)
        d = f.add(EXACT, DAEMON_DIAG)
        mine, other = c.collect([f.reports], f.marker, EXE, BID, f.out)
        self.assertEqual((mine, other), (["AzMail-2026-11-28-073102.ips"], [EXACT]))
        self.assertEqual((f.out / "crash" / a.name).read_text(), APP_IPS)
        self.assertEqual((f.out / "crash-system" / EXACT).read_text(), DAEMON_DIAG)
        self.assertTrue(a.exists() and d.exists(), "a copy: nothing is moved or deleted")

    def test_no_report_leaves_no_folder_behind(self):
        f = Folder(self)
        self.assertEqual(c.collect([f.reports], f.marker, EXE, BID, f.out), ([], []))
        self.assertFalse((f.out / "crash").exists())
        self.assertFalse((f.out / "crash-system").exists())

    def test_a_name_found_in_two_folders_is_named_once(self):
        f = Folder(self)
        f.add("AzMail-2026-11-28-073102.ips", APP_IPS)
        second = Folder(self)
        second.add("AzMail-2026-11-28-073102.ips", APP_IPS)
        mine, _ = c.collect([f.reports, second.reports], f.marker, EXE, BID, f.out)
        self.assertEqual(mine, ["AzMail-2026-11-28-073102.ips"])


class WaitTests(unittest.TestCase):
    def clock(self):
        self.now = 0.0
        self.sleeps = []

        def sleep(seconds):
            self.sleeps.append(seconds)
            self.now += seconds
        return sleep, lambda: self.now

    def test_a_system_report_does_not_end_the_wait_for_the_apps(self):
        f = Folder(self)
        f.add(EXACT, DAEMON_DIAG)
        sleep, clock = self.clock()
        c.wait_for_the_apps([f.reports], f.marker, EXE, BID, 20, sleep=sleep, clock=clock)
        self.assertEqual(sum(self.sleeps), 20, "waited the whole time: no report of the app came")

    def test_the_apps_report_that_comes_late_ends_the_wait(self):
        f = Folder(self)
        f.add(EXACT, DAEMON_DIAG)
        sleep, clock = self.clock()

        def writes_the_report_on_the_second_look(seconds):
            sleep(seconds)
            if len(self.sleeps) == 2:
                f.add("AzMail-2026-11-28-073110.ips", APP_IPS)
        c.wait_for_the_apps([f.reports], f.marker, EXE, BID, 20, sleep=writes_the_report_on_the_second_look, clock=clock)
        self.assertEqual(len(self.sleeps), 2)

    def test_a_report_that_is_already_there_is_not_waited_for(self):
        f = Folder(self)
        f.add("AzMail-2026-11-28-073110.ips", APP_IPS)
        sleep, clock = self.clock()
        c.wait_for_the_apps([f.reports], f.marker, EXE, BID, 20, sleep=sleep, clock=clock)
        self.assertEqual(self.sleeps, [])


class ShellTests(unittest.TestCase):
    def run_cli(self, f, *extra):
        return subprocess.run([sys.executable, "-I", str(HERE / "crash_reports.py"), "--exe", EXE, "--id", BID, "--since", str(f.marker),
                               "--out", str(f.out), *extra, str(f.reports)], capture_output=True, text=True)

    def evaluated(self, f, *extra):
        """What `eval "$(crash_reports.py ... --shell)"` leaves in the shell: crash_app, crash_other, crash_extra."""
        done = self.run_cli(f, "--shell", *extra)
        self.assertEqual(done.returncode, 0, done.stderr)
        script = f'eval "$1"; printf "%s\\n" "$crash_app" "$crash_other" "$crash_extra"'
        out = subprocess.run(["bash", "-c", script, "x", done.stdout], capture_output=True, text=True, check=True).stdout
        return out.split("\n")[:3], done.stderr

    def test_the_daemons_report_alone_leaves_no_crash_for_the_verdict_and_an_information_line(self):
        f = Folder(self)
        f.add(EXACT, DAEMON_DIAG)
        (app, other, extra), info = self.evaluated(f)
        self.assertEqual(app, "")
        self.assertEqual(other, EXACT)
        self.assertEqual(extra, '{"system_reports": ["%s"]}' % EXACT)
        self.assertIn("reports of other processes", info)
        self.assertIn(EXACT, info)
        self.assertIn("not part of the verdict", info)

    def test_the_apps_report_is_the_first_name_and_the_daemons_goes_on_the_information_line(self):
        f = Folder(self)
        f.add(EXACT, DAEMON_DIAG)
        f.add("AzMail-2026-11-28-073102.ips", APP_IPS)
        (app, other, _), info = self.evaluated(f)
        self.assertEqual((app, other), ("AzMail-2026-11-28-073102.ips", EXACT))
        self.assertIn("reports of the app: AzMail-2026-11-28-073102.ips", info)

    def test_no_report_at_all(self):
        f = Folder(self)
        (app, other, extra), info = self.evaluated(f)
        self.assertEqual((app, other, extra), ("", "", "{}"))
        self.assertIn("reports of the app: none", info)

    def test_a_file_name_with_quotes_and_a_command_in_it_reaches_the_shell_as_a_name(self):
        f = Folder(self)
        name = "AzMail-2026-11-28-073102 $(echo pwned)'x.ips"
        f.add(name, APP_IPS)
        (app, _, _), _ = self.evaluated(f)
        self.assertEqual(app, name)

    def test_the_wait_option_ends_at_once_when_the_apps_report_is_there(self):
        f = Folder(self)
        f.add("AzMail-2026-11-28-073102.ips", APP_IPS)
        done = self.run_cli(f, "--wait", "20")
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_a_marker_that_is_not_there_is_refused(self):
        f = Folder(self)
        f.marker.unlink()
        self.assertNotEqual(self.run_cli(f).returncode, 0)


class VerdictTests(unittest.TestCase):
    def test_system_reports_in_the_result_are_information_the_verdict_does_not_read(self):
        facts = {"alive": True, "alive_after": True, "screen_nonblank": True, "differs_from_home": True, "home_similarity": 0.012,
                 "fatal": None, "system_reports": [EXACT]}
        self.assertEqual(smoke_verdict.judge(facts)[0], True)

    def test_a_crash_of_the_app_is_still_fatal(self):
        facts = {"alive": True, "alive_after": True, "screen_nonblank": True, "differs_from_home": True, "home_similarity": 0.012,
                 "fatal": "crash report: AzMail-2026-11-28-073102.ips"}
        passed, reason = smoke_verdict.judge(facts)
        self.assertFalse(passed)
        self.assertIn("AzMail-2026-11-28-073102.ips", reason)


class BootTestScriptTests(unittest.TestCase):
    TEXT = (HERE / "ios_smoke.sh").read_text(encoding="utf-8")

    def test_the_boot_test_asks_the_helper_whose_a_report_is(self):
        self.assertIn("crash_reports.py", self.TEXT)
        self.assertIn('--exe "$exe" --id "$bid" --since "$marker"', self.TEXT)
        self.assertIn("--wait 20", self.TEXT)
        self.assertIn("--extra", self.TEXT)

    def test_no_file_of_the_hosts_folder_is_a_crash_just_because_it_is_new(self):
        self.assertNotIn("-exec cp", self.TEXT)
        self.assertNotIn("grep -v '^_ls.txt$'", self.TEXT)

    def test_the_script_is_valid_shell(self):
        self.assertEqual(subprocess.run(["bash", "-n", str(HERE / "ios_smoke.sh")]).returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
