//! Failing seeds kept as regression tests (feature (b) of the brief; planning/architecture.md section
//! 1.4 rule 2: a failing seed replays exactly).
//!
//! A `.seed` file is strict, one fact per line, `steps:` last:
//!
//! ```text
//! azlin-sim-seed v1
//! name: restore-lost-timeline-head
//! stage: 1
//! seed: 0x00000000000001a4
//! config: profile=restore-heavy scheme=fake accounts=1 members=2 devices=2 spaces=1 steps=250
//! invariant: inv_restore_the_lost_timeline_stays_lost
//! expect: pass
//! note: found by the long mode; fixed by <commit> in azlin-chain; X-DELTA-W1SIM-n
//! steps: 7
//! append dev=0x01 chain=0x00 payload=0x0003 salt=0x0000a1b2
//! ```
//!
//! `expect: pass` is a fixed defect or a canary: the replay must be green. `expect: fails` is an OPEN
//! defect (the note names its drift row): the replay must fail with exactly this invariant; if it
//! passes, the test fails and says "the defect looks fixed: change the file to `expect: pass`". So
//! an open defect never turns every hold red, and a fix cannot be forgotten.

use std::collections::BTreeMap;

use crate::config::{Config, ConfigError, FaultRates, LeaderVariant, Profile, Scheme, Stage};
use crate::invariants::InvariantId;
use crate::scenario::{Scenario, Verdict};
use crate::step::{Step, StepParseError};

/// What a seed file expects of its replay.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `SeedFile`, tests/seeds.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// The replay must be green (a fixed defect, a canary).
    Pass,
    /// The replay must fail with exactly the file's invariant (an open defect).
    Fails,
}

/// Why a seed file is not a seed file. One variant for each way to be wrong.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `SeedFile::parse`, tests/x02.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedParseError {
    /// The file is empty or its first line is not `azlin-sim-seed v1`.
    BadHeader,
    /// A key is missing (the file ended or the next line is another key).
    MissingKey(&'static str),
    /// A line that is not `key: value`, or a key that no seed file has.
    UnknownKey(String),
    /// A key given again.
    RepeatedKey(String),
    /// The name is empty or has a character outside `a-z 0-9 - _`.
    BadName,
    /// The stage is not 1 to 4.
    BadStage,
    /// The seed is not `0x` and sixteen lower-case hex digits.
    BadSeed,
    /// The configuration line is malformed (the key it fails at).
    BadConfig(String),
    /// The configuration is outside its limits.
    ConfigOutOfRange(ConfigError),
    /// No invariant of this name.
    UnknownInvariant,
    /// `expect` is neither `pass` nor `fails`.
    BadExpect,
    /// The number of steps is not a number.
    BadCount,
    /// The number of steps does not match the lines that follow.
    StepCountMismatch {
        /// What `steps:` says.
        declared: usize,
        /// The lines found.
        found: usize,
    },
    /// A step line is malformed (its index and why).
    BadStep(usize, StepParseError),
    /// Text after the last step.
    TrailingText,
}

error_text!(SeedParseError {
    SeedParseError::BadHeader => "the file does not start with `azlin-sim-seed v1`",
    SeedParseError::MissingKey(key) => "the key `{key}` is missing",
    SeedParseError::UnknownKey(line) => "unknown key or malformed line: {line}",
    SeedParseError::RepeatedKey(key) => "the key `{key}` is given twice",
    SeedParseError::BadName => "the name is empty or has a character outside a-z 0-9 - _",
    SeedParseError::BadStage => "the stage is not 1 to 4",
    SeedParseError::BadSeed => "the seed is not 0x and sixteen lower-case hex digits",
    SeedParseError::BadConfig(key) => "the configuration is malformed at `{key}`",
    SeedParseError::ConfigOutOfRange(error) => "the configuration is out of range: {error}",
    SeedParseError::UnknownInvariant => "no invariant of this name",
    SeedParseError::BadExpect => "`expect` is neither `pass` nor `fails`",
    SeedParseError::BadCount => "the number of steps is not a number",
    SeedParseError::StepCountMismatch { declared, found } => "`steps:` says {declared}, {found} lines follow",
    SeedParseError::BadStep(index, error) => "step line {index}: {error}",
    SeedParseError::TrailingText => "text after the last step",
});

/// A parsed seed file.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `replay_store`, `run_seeds`, tests/seeds.rs, tests/x02.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedFile {
    /// The name (also the stem of the file).
    pub name: String,
    /// The stage.
    pub stage: Stage,
    /// The seed.
    pub seed: u64,
    /// The configuration.
    pub config: Config,
    /// The invariant the file is about.
    pub invariant: InvariantId,
    /// What the replay must do.
    pub expect: Expect,
    /// A line of free text: the drift row or the commit.
    pub note: String,
    /// The steps.
    pub steps: Vec<Step>,
}

/// What replaying a seed file found.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `SeedFile::replay`, `replay_store`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedVerdict {
    /// The file says what the replay did.
    Ok,
    /// `expect: pass` and the replay was red.
    Failed(String),
    /// `expect: fails` and the replay was green.
    LooksFixed,
    /// `expect: fails` and the replay failed with another invariant.
    WrongInvariant(String),
    /// The scenario could not be built.
    Invalid(String),
}

fn expect_name(expect: Expect) -> &'static str {
    match expect {
        Expect::Pass => "pass",
        Expect::Fails => "fails",
    }
}

fn parse_config(text: &str, stage: Stage) -> Result<Config, SeedParseError> {
    let mut tokens = text.split(' ');
    let mut take = |key: &str| -> Result<String, SeedParseError> {
        let token = tokens
            .next()
            .ok_or_else(|| SeedParseError::BadConfig(key.to_string()))?;
        let (name, value) = token
            .split_once('=')
            .ok_or_else(|| SeedParseError::BadConfig(key.to_string()))?;
        if name != key {
            return Err(SeedParseError::BadConfig(key.to_string()));
        }
        Ok(value.to_string())
    };
    let number = |value: String, key: &str| -> Result<u32, SeedParseError> {
        if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
            return Err(SeedParseError::BadConfig(key.to_string()));
        }
        value
            .parse::<u32>()
            .map_err(|_| SeedParseError::BadConfig(key.to_string()))
    };
    let profile = Profile::from_name(&take("profile")?)
        .ok_or_else(|| SeedParseError::BadConfig(String::from("profile")))?;
    let scheme = Scheme::from_name(&take("scheme")?)
        .ok_or_else(|| SeedParseError::BadConfig(String::from("scheme")))?;
    let small = |value: u32, key: &str| -> Result<u8, SeedParseError> {
        u8::try_from(value).map_err(|_| SeedParseError::BadConfig(key.to_string()))
    };
    let accounts = small(number(take("accounts")?, "accounts")?, "accounts")?;
    let members = small(number(take("members")?, "members")?, "members")?;
    let devices = small(number(take("devices")?, "devices")?, "devices")?;
    let spaces = small(number(take("spaces")?, "spaces")?, "spaces")?;
    let steps = number(take("steps")?, "steps")?;
    let mut leader = LeaderVariant::Honest;
    let mut leader_token = tokens.next();
    if let Some(token) = leader_token {
        if let Some(value) = token.strip_prefix("leader=") {
            leader = LeaderVariant::from_name(value)
                .ok_or_else(|| SeedParseError::BadConfig(String::from("leader")))?;
            if leader == LeaderVariant::Honest {
                return Err(SeedParseError::BadConfig(String::from("leader")));
            }
            leader_token = None;
        }
    }
    let mut start_epoch = 0u64;
    let mut next = leader_token;
    if next.is_none() {
        next = tokens.next();
    }
    if let Some(token) = next {
        let value = token
            .strip_prefix("start_epoch=")
            .ok_or_else(|| SeedParseError::BadConfig(String::from("start_epoch")))?;
        if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
            return Err(SeedParseError::BadConfig(String::from("start_epoch")));
        }
        start_epoch = value
            .parse::<u64>()
            .map_err(|_| SeedParseError::BadConfig(String::from("start_epoch")))?;
        if start_epoch == 0 {
            return Err(SeedParseError::BadConfig(String::from("start_epoch")));
        }
    }
    if tokens.next().is_some() {
        return Err(SeedParseError::BadConfig(String::from("trailing")));
    }
    let config = Config {
        stage,
        profile,
        scheme,
        accounts,
        members,
        devices,
        spaces,
        steps,
        faults: FaultRates::for_profile(profile),
        leader,
        start_epoch,
    };
    config
        .validate()
        .map_err(SeedParseError::ConfigOutOfRange)?;
    Ok(config)
}

impl SeedFile {
    /// The text of the file.
    pub fn to_text(&self) -> String {
        let mut out = String::from("azlin-sim-seed v1\n");
        out.push_str(&format!("name: {}\n", self.name));
        out.push_str(&format!("stage: {}\n", self.stage.number()));
        out.push_str(&format!("seed: 0x{:016x}\n", self.seed));
        out.push_str(&format!("config: {}\n", self.config));
        out.push_str(&format!("invariant: {}\n", self.invariant.name()));
        out.push_str(&format!("expect: {}\n", expect_name(self.expect)));
        out.push_str(&format!("note: {}\n", self.note));
        out.push_str(&format!("steps: {}\n", self.steps.len()));
        for step in &self.steps {
            out.push_str(&step.to_line());
            out.push('\n');
        }
        out
    }

    /// Parses a file; anything that is not exactly the format is a typed error.
    pub fn parse(text: &str) -> Result<SeedFile, SeedParseError> {
        let mut lines = text.split('\n');
        if lines.next() != Some("azlin-sim-seed v1") {
            return Err(SeedParseError::BadHeader);
        }
        let mut fields: BTreeMap<&'static str, String> = BTreeMap::new();
        const KEYS: [&str; 8] = [
            "name",
            "stage",
            "seed",
            "config",
            "invariant",
            "expect",
            "note",
            "steps",
        ];
        for key in KEYS {
            let line = lines.next().ok_or(SeedParseError::MissingKey(key))?;
            let Some((name, value)) = line.split_once(": ").or_else(|| line.split_once(':')) else {
                return Err(SeedParseError::UnknownKey(line.to_string()));
            };
            if name != key {
                if KEYS.contains(&name) {
                    return Err(SeedParseError::RepeatedKey(name.to_string()));
                }
                return Err(SeedParseError::UnknownKey(line.to_string()));
            }
            fields.insert(key, value.to_string());
        }
        let get = |key: &'static str| {
            fields
                .get(key)
                .cloned()
                .ok_or(SeedParseError::MissingKey(key))
        };
        let name = get("name")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        {
            return Err(SeedParseError::BadName);
        }
        let stage_number = get("stage")?
            .parse::<u8>()
            .map_err(|_| SeedParseError::BadStage)?;
        let stage = Stage::from_number(stage_number).ok_or(SeedParseError::BadStage)?;
        let seed_text = get("seed")?;
        let digits = seed_text
            .strip_prefix("0x")
            .ok_or(SeedParseError::BadSeed)?;
        if digits.len() != 16
            || !digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(SeedParseError::BadSeed);
        }
        let seed = u64::from_str_radix(digits, 16).map_err(|_| SeedParseError::BadSeed)?;
        let config = parse_config(&get("config")?, stage)?;
        let invariant =
            InvariantId::from_name(&get("invariant")?).ok_or(SeedParseError::UnknownInvariant)?;
        let expect = match get("expect")?.as_str() {
            "pass" => Expect::Pass,
            "fails" => Expect::Fails,
            _ => return Err(SeedParseError::BadExpect),
        };
        let note = get("note")?;
        let declared = get("steps")?
            .parse::<usize>()
            .map_err(|_| SeedParseError::BadCount)?;
        let mut steps = Vec::new();
        let mut rest: Vec<&str> = lines.collect();
        // The text ends with a newline: the last piece is empty.
        if rest.last() == Some(&"") {
            rest.pop();
        }
        if rest.len() != declared {
            return Err(SeedParseError::StepCountMismatch {
                declared,
                found: rest.len(),
            });
        }
        for (index, line) in rest.iter().enumerate() {
            steps.push(Step::parse(line).map_err(|error| SeedParseError::BadStep(index, error))?);
        }
        Ok(SeedFile {
            name,
            stage,
            seed,
            config,
            invariant,
            expect,
            note,
            steps,
        })
    }

    /// The scenario of this file.
    pub fn scenario(&self) -> Result<Scenario, ConfigError> {
        Scenario::new(self.seed, self.config.clone())
    }

    /// Replays the file against the current code and says whether it did what the file says.
    pub fn replay(&self) -> SeedVerdict {
        let scenario = match self.scenario() {
            Ok(scenario) => scenario,
            Err(error) => return SeedVerdict::Invalid(error.to_string()),
        };
        let outcome = scenario.replay(&self.steps);
        let failed_with: Option<InvariantId> = match &outcome.verdict {
            Verdict::Green => None,
            Verdict::Red(failure) => Some(failure.invariant),
        };
        match self.expect {
            Expect::Pass => match &outcome.verdict {
                Verdict::Green => SeedVerdict::Ok,
                Verdict::Red(failure) => SeedVerdict::Failed(format!(
                    "{} failed at step {}: {}",
                    failure.invariant.name(),
                    failure.at_step,
                    failure.message
                )),
            },
            Expect::Fails => match failed_with {
                None => SeedVerdict::LooksFixed,
                Some(found) => {
                    let also = outcome
                        .known_gaps
                        .iter()
                        .any(|gap| gap.invariant == self.invariant);
                    if found == self.invariant || also {
                        SeedVerdict::Ok
                    } else {
                        SeedVerdict::WrongInvariant(found.name().to_string())
                    }
                }
            },
        }
    }
}

/// Why a seed store refused.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `SeedStore`, `DirSeeds`, `MemSeeds`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// A file of this name exists (a store never overwrites).
    Exists,
    /// No file of this name.
    NotFound,
    /// The name is not a plain file name.
    BadName,
    /// The disk said no.
    Io(String),
}

error_text!(StoreError {
    StoreError::Exists => "a seed file of this name exists",
    StoreError::NotFound => "no seed file of this name",
    StoreError::BadName => "the name is not a plain file name",
    StoreError::Io(message) => "the seed store failed: {message}",
});

/// Where seed files live.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `run_seeds`, `replay_store`, src/bin/simrun.rs, tests/seeds.rs.
pub trait SeedStore {
    /// The names of the files, sorted.
    fn names(&self) -> Result<Vec<String>, StoreError>;
    /// The text of a file.
    fn read(&self, name: &str) -> Result<String, StoreError>;
    /// Creates a file; an existing one is `Exists`, never overwritten.
    fn create(&mut self, name: &str, text: &str) -> Result<(), StoreError>;
}

/// A seed store in memory.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: tests/x06.
#[derive(Clone, Debug, Default)]
pub struct MemSeeds {
    files: BTreeMap<String, String>,
}

impl MemSeeds {
    /// An empty store.
    pub fn new() -> MemSeeds {
        MemSeeds::default()
    }
}

/// Whether a name is a plain file name of a seed.
pub(crate) fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.'
        })
        && !name.starts_with('.')
}

impl SeedStore for MemSeeds {
    fn names(&self) -> Result<Vec<String>, StoreError> {
        Ok(self.files.keys().cloned().collect())
    }

    fn read(&self, name: &str) -> Result<String, StoreError> {
        self.files.get(name).cloned().ok_or(StoreError::NotFound)
    }

    fn create(&mut self, name: &str, text: &str) -> Result<(), StoreError> {
        if !plain_name(name) {
            return Err(StoreError::BadName);
        }
        if self.files.contains_key(name) {
            return Err(StoreError::Exists);
        }
        self.files.insert(name.to_string(), text.to_string());
        Ok(())
    }
}

/// What replaying a whole store found.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `replay_store`, tests/seeds.rs, tests/x06.
#[derive(Clone, Debug, Default)]
pub struct ReplayReport {
    /// Files checked.
    pub checked: usize,
    /// Files whose replay did what they say.
    pub ok: usize,
    /// Files that did not, with the reason.
    pub failures: Vec<(String, String)>,
    /// `expect: fails` files whose defect looks fixed.
    pub looks_fixed: Vec<String>,
}

impl ReplayReport {
    /// Whether every file did what it says and none looks fixed.
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty() && self.looks_fixed.is_empty()
    }
}

/// Replays every file of a store against the current code.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: tests/seeds.rs, tests/x06, src/bin/simrun.rs.
pub fn replay_store(store: &dyn SeedStore) -> ReplayReport {
    let mut report = ReplayReport::default();
    let names = match store.names() {
        Ok(names) => names,
        Err(error) => {
            report
                .failures
                .push((String::from("<store>"), error.to_string()));
            return report;
        }
    };
    for name in names {
        report.checked += 1;
        let text = match store.read(&name) {
            Ok(text) => text,
            Err(error) => {
                report.failures.push((name, error.to_string()));
                continue;
            }
        };
        let file = match SeedFile::parse(&text) {
            Ok(file) => file,
            Err(error) => {
                report.failures.push((name, format!("malformed: {error}")));
                continue;
            }
        };
        match file.replay() {
            SeedVerdict::Ok => report.ok += 1,
            SeedVerdict::LooksFixed => {
                report.looks_fixed.push(format!(
                    "{name}: the defect looks fixed: change the file to `expect: pass`"
                ));
            }
            SeedVerdict::Failed(reason)
            | SeedVerdict::WrongInvariant(reason)
            | SeedVerdict::Invalid(reason) => {
                report.failures.push((name, reason));
            }
        }
    }
    report
}
