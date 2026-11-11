//! What only a TEST run may do (ARCH28 RF38; decision of 2026-11-04: a shipped build reads no test
//! switch).
//!
//! A test run of an azul app replaces a few things that talk to the machine: the keyring is a map
//! in memory or a plaintext file, a sign-in is answered by a variable, `Url::open` opens nothing,
//! the biometric prompt says "unavailable", the battery and the network read fixed values, speech
//! says nothing, and file dialogs, color pickers and HTTP requests are answered from a canned
//! table. Each of those is right in a test and a hole in a user's run: a variable in a shell
//! profile, a launcher or a wrapper script would send a user's drive keys to process memory (lost
//! at exit) or to a plaintext file, answer a sign-in without the user, or let a key change through
//! a presence check that was never made.
//!
//! So they are asked for in ONE place. [`test_hooks`] is the hooks of this process:
//!
//! - in a build with the cargo feature `test-hooks` (the unit tests of the crates that need it and
//!   the E2E builds: `--features build-dll,test-hooks`), what the environment asks for -
//!   `AZ_BACKEND=headless` as a STAND-IN switch (everywhere else it stays what it always was: a
//!   windowing choice), `AZ_E2E_TEST`, `AZ_E2E`, `AZ_KEYRING`, `AZ_KEYRING_FILE` and
//!   `AZ_AUTH_SESSION_REDIRECT`;
//! - in every other build, [`TestHooks::NONE`], whatever the environment holds: the variables are
//!   not even read.
//!
//! A consumer asks the hook it needs (`test_hooks().open_nothing`, ...) and never reads one of
//! those variables itself; `dll/tests/run_mode_gate.rs` fails when one does. The hooks are read
//! once, at the first question: a test that must see another environment builds its own
//! [`TestHooks`] ([`TestHooks::from_env`] takes the environment as a function) and hands it to
//! the pure function behind the consumer.

use std::ffi::OsString;

/// The variable that picks a test run's keyring: `system`, `file` or `memory`.
pub const KEYRING_CHOICE_VAR: &str = "AZ_KEYRING";

/// The variable naming the file that keeps a test stack's keyring instead of the system's.
pub const KEYRING_FILE_VAR: &str = "AZ_KEYRING_FILE";

/// The scenario file of a scripted test run (any value, an empty one too, makes the run a test
/// run for the stand-ins; the request mocks need a non-empty one).
pub const SCENARIO_VAR: &str = "AZ_E2E_TEST";

/// The script of an end-to-end run (the request mocks need a non-empty one).
pub const SCRIPT_VAR: &str = "AZ_E2E";

/// The URL a test run's sign-in comes back to, with `{redirect_uri}`, `{state}` and
/// `{code_challenge}` replaced by the request's; `cancel` or `timeout` for those answers.
pub const FAKE_REDIRECT_VAR: &str = "AZ_AUTH_SESSION_REDIRECT";

/// The windowing choice. `headless` is a stand-in switch here and nowhere else.
const BACKEND_VAR: &str = "AZ_BACKEND";

/// Whether this build can have hooks at all: the cargo feature `test-hooks`. A shipped build
/// (`dist`) is built without it.
pub const AVAILABLE: bool = cfg!(feature = "test-hooks");

/// The stand-ins a test run turns on.
///
/// Every field is its own switch (a test of one consumer builds a value with just that field);
/// the environment turns them on together, as the old per-consumer reads did.
// Each field is one independent switch of a test: a state machine or a set of enums would only
// rename them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestHooks {
    /// `AZ_KEYRING` as typed: `system`, `file` or `memory` (any case); another word chooses
    /// nothing.
    pub keyring_choice: Option<OsString>,
    /// `AZ_KEYRING_FILE` as typed: the file that keeps the stand-in keyring (a blank name is
    /// none).
    pub keyring_file: Option<OsString>,
    /// The run never reaches the system keyring (a test on a developer's Mac would raise the
    /// keychain's prompts).
    pub keyring_headless: bool,
    /// A sign-in opens no browser and is answered by [`FAKE_REDIRECT_VAR`] (`fake_redirect`).
    pub fake_sign_in: bool,
    /// `AZ_AUTH_SESSION_REDIRECT` as typed; read only in a run with `fake_sign_in`.
    pub fake_redirect: Option<String>,
    /// `Url::open` starts nothing and answers "opened".
    pub open_nothing: bool,
    /// The biometric prompt answers "unavailable" and shows no sheet. `presence::decide` of
    /// `AzDrive` runs the action of an unavailable sensor ONLY when this is on.
    pub no_biometric: bool,
    /// The device-state readings (battery, network, power) are fixed values or a test's switch
    /// file, never the machine's own.
    pub no_sensors: bool,
    /// Nothing is said or heard: utterances are logged, the heard text is the test's.
    pub no_speech: bool,
    /// The request mock store (file dialogs, color pickers, HTTP) is armed by the environment.
    pub mocks: bool,
    /// A SCRIPT drives the run (`AZ_E2E` or `AZ_E2E_TEST`, non-empty), rather than a bare
    /// headless launch: unmocked HTTP requests are refused unless a pattern allows them.
    pub scripted: bool,
}

impl TestHooks {
    /// No hook: what every shipped build has.
    pub const NONE: Self = Self {
        keyring_choice: None,
        keyring_file: None,
        keyring_headless: false,
        fake_sign_in: false,
        fake_redirect: None,
        open_nothing: false,
        no_biometric: false,
        no_sensors: false,
        no_speech: false,
        mocks: false,
        scripted: false,
    };

    /// Every stand-in of a headless run on, nothing chosen: what `AZ_BACKEND=headless` alone asks
    /// for (for the tests of the consumers).
    pub const HEADLESS: Self = Self {
        keyring_choice: None,
        keyring_file: None,
        keyring_headless: true,
        fake_sign_in: true,
        fake_redirect: None,
        open_nothing: true,
        no_biometric: true,
        no_sensors: true,
        no_speech: true,
        mocks: true,
        scripted: false,
    };

    /// The hooks a run asks for whose environment is `var`. Pure: this is the one place that
    /// knows which variable means what, and a test hands it an environment of its own.
    ///
    /// - A STAND-IN run is `AZ_BACKEND=headless` or ANY value of `AZ_E2E_TEST` (an empty one
    ///   too): the keyring, the sign-in, `Url::open`, the biometric prompt, the sensors and the
    ///   speech all answer from their stand-ins.
    /// - The request mocks are armed by the same, or by a non-empty `AZ_E2E`; `scripted` is a
    ///   non-empty `AZ_E2E` or `AZ_E2E_TEST` without the bare headless launch.
    #[must_use]
    pub fn from_env(var: &dyn Fn(&str) -> Option<OsString>) -> Self {
        let non_empty = |name: &str| var(name).is_some_and(|value| !value.is_empty());
        let headless = var(BACKEND_VAR).is_some_and(|backend| backend == "headless");
        let stand_in = headless || var(SCENARIO_VAR).is_some();
        let scripted = non_empty(SCRIPT_VAR) || non_empty(SCENARIO_VAR);
        Self {
            keyring_choice: var(KEYRING_CHOICE_VAR),
            keyring_file: var(KEYRING_FILE_VAR),
            keyring_headless: stand_in,
            fake_sign_in: stand_in,
            fake_redirect: var(FAKE_REDIRECT_VAR).and_then(|value| value.into_string().ok()),
            open_nothing: stand_in,
            no_biometric: stand_in,
            no_sensors: stand_in,
            no_speech: stand_in,
            mocks: scripted || headless,
            scripted,
        }
    }
}

/// The words a libazul built with the cargo feature `test-hooks` carries in its binary (a `#[used]`
/// static that [`test_hooks`] reads): such a build takes its keyring, its sign-in, `Url::open`, its
/// biometric prompt, its sensors and its launch from environment variables, so nothing signed and
/// shipped may be one - and cargo unifies features across a workspace build, so a binary can be one
/// by accident. The prefix is the one `scripts/sign-release.sh release` refuses and
/// `scripts/check_no_test_hooks.py` looks for (azul-appkit's `anchors::TEST_HOOKS_MARKER` carries it
/// for its own `test-hooks`); `scripts/azlin_e2e.py` builds libazul with the feature on purpose
/// (X-DELTA-AZDRIVE-KEYS-37). A shipped build never refers to the constant, so its binary does not
/// hold the bytes.
pub const TEST_HOOKS_MARKER: &str =
    "AZLIN-BUILD-MARKER:test-hooks:libazul-reads-its-stand-ins-from-the-environment";

/// The marker in a test build's binary.
#[cfg(feature = "test-hooks")]
#[used]
static TEST_HOOKS_IN_THIS_BINARY: &str = TEST_HOOKS_MARKER;

/// The hooks of this process: [`TestHooks::from_env`] of its environment, read at the first call,
/// in a build with the cargo feature `test-hooks`.
#[cfg(feature = "test-hooks")]
#[must_use]
pub fn test_hooks() -> &'static TestHooks {
    static HOOKS: std::sync::OnceLock<TestHooks> = std::sync::OnceLock::new();
    // Every binary that asks for a hook keeps the marker (a linker that drops what nothing reads
    // would drop it otherwise).
    std::hint::black_box(TEST_HOOKS_IN_THIS_BINARY);
    HOOKS.get_or_init(|| TestHooks::from_env(&|name| std::env::var_os(name)))
}

/// The hooks of this process: none. This build has no cargo feature `test-hooks`, so no variable
/// is read and whatever the environment holds is not a test switch.
#[cfg(not(feature = "test-hooks"))]
#[must_use]
pub const fn test_hooks() -> &'static TestHooks {
    &TestHooks::NONE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment `pairs` describe.
    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(*value))
        }
    }

    fn hooks(pairs: &'static [(&'static str, &'static str)]) -> TestHooks {
        TestHooks::from_env(&env(pairs))
    }

    /// X-DELTA-AZDRIVE-KEYS-37: libazul's own `test-hooks` carries the marker the release script
    /// refuses and the CI step looks for, with the prefix the scripts share with azul-appkit's.
    #[test]
    fn x_delta_azdrive_keys_37_libazuls_test_hooks_carry_the_marker_the_release_script_refuses() {
        assert!(TEST_HOOKS_MARKER.starts_with("AZLIN-BUILD-MARKER:test-hooks:"));
        #[cfg(feature = "test-hooks")]
        {
            assert_eq!(TEST_HOOKS_IN_THIS_BINARY, TEST_HOOKS_MARKER);
            // `test_hooks()` keeps it in every binary that asks for a hook.
            let _ = test_hooks();
        }
    }

    /// An environment without a test switch asks for no hook at all.
    #[test]
    fn a_plain_environment_asks_for_no_hook() {
        assert_eq!(hooks(&[]), TestHooks::NONE);
        // The windowing choices that are not the headless stand-in, and variables of other
        // programs.
        assert_eq!(hooks(&[("AZ_BACKEND", "metal")]), TestHooks::NONE);
        assert_eq!(hooks(&[("AZ_BACKEND", "cpu")]), TestHooks::NONE);
        assert_eq!(
            hooks(&[("AZ_BACKEND", "web://0.0.0.0:8080")]),
            TestHooks::NONE
        );
        assert_eq!(hooks(&[("AZ_BACKEND", "")]), TestHooks::NONE);
        assert_eq!(hooks(&[("AZ_BACKEND", "Headless")]), TestHooks::NONE);
        assert_eq!(
            hooks(&[("AZ_DEBUG", "8765"), ("HOME", "/home/ada")]),
            TestHooks::NONE
        );
    }

    /// `AZ_BACKEND=headless` turns every stand-in on and arms the mocks, but no script drives it.
    #[test]
    fn a_headless_launch_turns_every_stand_in_on_and_arms_the_mocks_without_a_script() {
        assert_eq!(hooks(&[("AZ_BACKEND", "headless")]), TestHooks::HEADLESS);
        let headless = TestHooks::HEADLESS;
        assert!(headless.keyring_headless && headless.fake_sign_in && headless.open_nothing);
        assert!(headless.no_biometric && headless.no_sensors && headless.no_speech);
        assert!(headless.mocks && !headless.scripted);
    }

    /// Any value of `AZ_E2E_TEST` is a stand-in run, as every consumer read it before - the empty
    /// one too; the request mocks asked for a non-empty one.
    #[test]
    fn any_value_of_the_scenario_variable_is_a_stand_in_run_but_only_a_non_empty_one_scripts() {
        let full = hooks(&[("AZ_E2E_TEST", "/tmp/scenario.json")]);
        assert!(full.keyring_headless && full.fake_sign_in && full.open_nothing);
        assert!(full.no_biometric && full.no_sensors && full.no_speech);
        assert!(full.mocks && full.scripted);

        let empty = hooks(&[("AZ_E2E_TEST", "")]);
        assert!(empty.keyring_headless && empty.fake_sign_in && empty.open_nothing);
        assert!(empty.no_biometric && empty.no_sensors && empty.no_speech);
        assert!(!empty.mocks, "the mocks asked for a non-empty AZ_E2E_TEST");
        assert!(!empty.scripted);
    }

    /// `AZ_E2E` arms the mocks of a script, nothing else: it never made a run headless for the
    /// keyring, the sign-in, the opener, the biometric prompt, the sensors or the speech.
    #[test]
    fn a_script_arms_the_mocks_and_no_stand_in() {
        let script = hooks(&[("AZ_E2E", "/tmp/script.json")]);
        assert!(script.mocks && script.scripted);
        assert!(!script.keyring_headless && !script.fake_sign_in && !script.open_nothing);
        assert!(!script.no_biometric && !script.no_sensors && !script.no_speech);
        assert_eq!(
            hooks(&[("AZ_E2E", "")]),
            TestHooks::NONE,
            "an empty AZ_E2E is none"
        );
    }

    /// A script on a headless launch: the stand-ins and the mocks, and the script drives it.
    #[test]
    fn a_script_on_a_headless_launch_is_a_scripted_stand_in_run() {
        let both = hooks(&[("AZ_BACKEND", "headless"), ("AZ_E2E", "/tmp/s.json")]);
        assert!(both.fake_sign_in && both.mocks && both.scripted);
    }

    /// The keyring's choice and file and the sign-in's answer are carried as they were typed.
    #[test]
    fn the_keyring_switches_and_the_fake_redirect_are_carried_as_typed() {
        let h = hooks(&[
            ("AZ_KEYRING", " File "),
            ("AZ_KEYRING_FILE", "/tmp/a-stack/keyring.json"),
            (
                "AZ_AUTH_SESSION_REDIRECT",
                "{redirect_uri}?code=x&state={state}",
            ),
        ]);
        assert_eq!(h.keyring_choice, Some(OsString::from(" File ")));
        assert_eq!(
            h.keyring_file,
            Some(OsString::from("/tmp/a-stack/keyring.json"))
        );
        assert_eq!(
            h.fake_redirect.as_deref(),
            Some("{redirect_uri}?code=x&state={state}")
        );
        // Naming a keyring file is not a headless run: a windowed run keeps its keyring there.
        assert!(!h.keyring_headless && !h.fake_sign_in);
    }

    /// A switch is looked at by its exact name.
    #[test]
    fn only_the_exact_names_are_switches() {
        for name in [
            "az_backend",
            "AZ_BACKEND_",
            "AZ_E2E_TESTS",
            "AZ_KEYRING_FILES",
            "AZ_AUTH_SESSION_REDIRECTS",
        ] {
            let var = move |asked: &str| (asked == name).then(|| OsString::from("headless"));
            assert_eq!(TestHooks::from_env(&var), TestHooks::NONE, "{name}");
        }
    }

    /// A build without the cargo feature has no hook, whatever the environment is: this is the
    /// answer every consumer gets in a shipped build. (The process-level proof, with the
    /// variables really set, is `tests/run_mode_release.rs`.)
    #[cfg(not(feature = "test-hooks"))]
    #[test]
    fn a_build_without_the_feature_has_no_hook() {
        assert!(!AVAILABLE);
        assert_eq!(test_hooks(), &TestHooks::NONE);
    }

    /// A build with the cargo feature reads its environment once and hands out the same hooks.
    #[cfg(feature = "test-hooks")]
    #[test]
    fn a_build_with_the_feature_reads_its_environment_once() {
        assert!(AVAILABLE);
        assert!(std::ptr::eq(test_hooks(), test_hooks()));
    }
}
