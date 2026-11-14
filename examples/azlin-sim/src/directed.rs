//! The directed scenarios (planning/architecture.md section 1.4 rule 2: a crash at any point; section
//! 2.17 and section 6.10 V3: the restore story; section 12 E4): hand-written step lists that run
//! green on the committed code, and the table of crash cases (one for each await point of table 4.4
//! of the brief, built from the handler table so that a new handler or phase needs its row).
//!
//! The scenarios use a small world (one account, two members with one device each, a team log and one
//! space) under the calm profile: nothing is lost, delayed or torn by chance; every fault is a step.

use crate::config::{Config, Profile, Scheme, Stage};
use crate::invariants::InvariantId;
use crate::scenario::{Outcome, Scenario};
use crate::sched::{JobKind, HANDLERS};
use crate::seeds::{Expect, SeedFile};
use crate::step::{AttackKind, DiskFault, SigChoice, Step};
use crate::time::Shift;

/// A builder of step lists with a fresh salt for every step.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
///
/// Caller: `directed`, tests/s1_*, tests/planted_*.
#[derive(Clone, Debug, Default)]
pub struct Steps {
    steps: Vec<Step>,
    salt: u32,
}

impl Steps {
    /// An empty list.
    pub fn new() -> Steps {
        Steps::default()
    }

    /// An empty list whose salts start from another point (a planted test tries several).
    pub fn with_salt(salt: u32) -> Steps {
        Steps {
            steps: Vec::new(),
            salt,
        }
    }

    fn next_salt(&mut self) -> u32 {
        self.salt = self.salt.wrapping_add(0x9E37_79B1);
        self.salt
    }

    /// The steps so far.
    pub fn build(self) -> Vec<Step> {
        self.steps
    }

    /// How many steps there are.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether there is no step.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// `append`.
    #[must_use]
    pub fn append(mut self, dev: u8, chain: u8, payload: u16) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Append {
            dev,
            chain,
            payload,
            salt,
        });
        self
    }

    /// `deliver`.
    #[must_use]
    pub fn deliver(mut self, n: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Deliver { n, salt });
        self
    }

    /// `poll`.
    #[must_use]
    pub fn poll(mut self, node: u8, n: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Poll { node, n, salt });
        self
    }

    /// `poll`, or nothing when `n` is 0 (a poll of 0 phases would run one).
    #[must_use]
    pub fn polls(self, node: u8, n: u8) -> Steps {
        if n == 0 {
            self
        } else {
            self.poll(node, n)
        }
    }

    /// `tick`.
    #[must_use]
    pub fn tick(mut self, secs: u32) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Tick { secs, salt });
        self
    }

    /// `head_tick`.
    #[must_use]
    pub fn head_tick(mut self) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::HeadTick { salt });
        self
    }

    /// `fetch`.
    #[must_use]
    pub fn fetch(mut self, dev: u8, chain: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Fetch { dev, chain, salt });
        self
    }

    /// `meet`.
    #[must_use]
    pub fn meet(mut self, a: u8, b: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Meet { a, b, salt });
        self
    }

    /// `checkpoint`.
    #[must_use]
    pub fn checkpoint(mut self, chain: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Checkpoint { chain, salt });
        self
    }

    /// `offsite`.
    #[must_use]
    pub fn offsite(mut self) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Offsite { salt });
        self
    }

    /// `disaster`.
    #[must_use]
    pub fn disaster(mut self, copy: u8, sig: SigChoice, announce: bool) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Disaster {
            copy,
            sig,
            announce,
            salt,
        });
        self
    }

    /// `resend`.
    #[must_use]
    pub fn resend(mut self, dev: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Resend { dev, salt });
        self
    }

    /// `partition`.
    #[must_use]
    pub fn partition(mut self, mask: u16) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Partition { mask, salt });
        self
    }

    /// `heal`.
    #[must_use]
    pub fn heal(mut self, mask: u16) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Heal { mask, salt });
        self
    }

    /// `skew`.
    #[must_use]
    pub fn skew(mut self, node: u8, secs: i32) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Skew {
            node,
            shift: Shift::from_secs(secs),
            salt,
        });
        self
    }

    /// `jump`.
    #[must_use]
    pub fn jump(mut self, node: u8, secs: i32) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Jump {
            node,
            shift: Shift::from_secs(secs),
            salt,
        });
        self
    }

    /// `crash`.
    #[must_use]
    pub fn crash(mut self, node: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Crash { node, salt });
        self
    }

    /// `restart`.
    #[must_use]
    pub fn restart(mut self, node: u8, wipe: bool) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Restart { node, wipe, salt });
        self
    }

    /// `disk`.
    #[must_use]
    pub fn disk(mut self, node: u8, fault: DiskFault) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Disk { node, fault, salt });
        self
    }

    /// `attack`.
    #[must_use]
    pub fn attack(mut self, kind: AttackKind, dev: u8, chain: u8) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Attack {
            kind,
            dev,
            chain,
            salt,
        });
        self
    }

    /// `policy`.
    #[must_use]
    pub fn policy(mut self, scope: u8, group: u8, patch: u32) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Policy {
            scope,
            group,
            patch,
            salt,
        });
        self
    }

    /// `rounds` rounds of the world going on by hand, without a `quiesce` (which would bring a
    /// crashed node back and heal what is cut): every node runs up to twelve phases, three seconds
    /// pass, and up to twelve messages are delivered.
    #[must_use]
    pub fn pump(mut self, rounds: u32) -> Steps {
        for _ in 0..rounds {
            self = self
                .poll(0, 12)
                .poll(1, 12)
                .poll(2, 12)
                .tick(3)
                .deliver(12);
        }
        self
    }

    /// `quiesce`.
    #[must_use]
    pub fn quiesce(mut self) -> Steps {
        let salt = self.next_salt();
        self.steps.push(Step::Quiesce { salt });
        self
    }
}

/// What a directed scenario must do.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `Directed`, tests/s1_11.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectedExpect {
    /// Green on the committed code.
    Green,
    /// Red with this invariant (a planted violation).
    Red(InvariantId),
}

/// One directed scenario.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: tests/s1_11, `mutants_table`.
#[derive(Clone, Copy, Debug)]
pub struct Directed {
    /// The name.
    pub name: &'static str,
    /// The configuration of its world.
    pub config: fn() -> Config,
    /// The steps.
    pub steps: fn() -> Vec<Step>,
    /// What it must do.
    pub expect: DirectedExpect,
}

/// The small world of the directed scenarios: one account, two members with one device each, a team
/// log and one space, under the calm profile with the fast scheme.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `DIRECTED`, tests.
pub fn small_world() -> Config {
    Config::standard(0, Stage::S1)
        .with_profile(Profile::Calm)
        .with_scheme(Scheme::Fake)
        .with_size(1, 2, 1, 1)
        .with_steps(1)
}

/// The seed a directed scenario runs under.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
///
/// Caller: `run_directed`, `Directed`, tests/seeds.rs, tests/fast.rs.
pub const DIRECTED_SEED: u64 = 0xD15E_C7ED;

fn restore_story() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .quiesce()
        .offsite()
        .append(0, 0, 3)
        .append(1, 1, 4)
        .quiesce()
        .disaster(0, SigChoice::Map, true)
        .quiesce()
        .build()
}

fn restore_then_hostile() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .quiesce()
        .offsite()
        .append(0, 0, 3)
        .append(1, 0, 4)
        .quiesce()
        .attack(AttackKind::GroupAloneRestore, 0, 0)
        .attack(AttackKind::RestoreWithoutSignature, 1, 0)
        .disaster(0, SigChoice::Map, true)
        .quiesce()
        .attack(AttackKind::ReplayOldRestore, 0, 0)
        .attack(AttackKind::LostTimelineHead, 1, 0)
        .attack(AttackKind::OldEpochEntry, 0, 0)
        .quiesce()
        .build()
}

fn offline_device_heals() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .quiesce()
        .offsite()
        .append(0, 0, 2)
        .quiesce()
        .crash(2)
        .disaster(0, SigChoice::Map, true)
        .append(0, 0, 5)
        .quiesce()
        .restart(2, false)
        .quiesce()
        .build()
}

fn new_device_after_restore() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .quiesce()
        .offsite()
        .append(0, 0, 3)
        .quiesce()
        .disaster(0, SigChoice::Map, true)
        .quiesce()
        .crash(2)
        .restart(2, true)
        .quiesce()
        .build()
}

fn checkpoints_by_age() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .quiesce()
        .tick(31 * 86_400)
        .head_tick()
        .quiesce()
        .build()
}

fn peers_and_withheld_heads() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .quiesce()
        // Device 0 (node 1) is down while device 1 appends: its notices are lost to it.
        .crash(1)
        .append(1, 0, 3)
        .poll(2, 1)
        .tick(3)
        .deliver(1)
        .poll(0, 6)
        .tick(3)
        .deliver(6)
        .poll(2, 8)
        .restart(1, false)
        // Device 0 is back with the older state; device 1 holds the newer head.
        .attack(AttackKind::StaleHeadToOne, 0, 0)
        .attack(AttackKind::PeerJunk, 0, 0)
        .attack(AttackKind::PeerFork, 1, 0)
        .meet(0, 1)
        .quiesce()
        .build()
}

fn restore_late_record_keeps_progress() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .quiesce()
        .offsite()
        .append(0, 0, 2)
        .quiesce()
        // Device 1 (node 2) is down when the restore is announced.
        .crash(2)
        .disaster(0, SigChoice::Map, true)
        .poll(0, 4)
        .tick(60)
        // Device 0 (node 1) follows the record and the new timeline grows beyond the position the
        // lost one had reached (three entries after the survivor, with the entry sent again).
        .pump(8)
        .append(0, 0, 3)
        .pump(8)
        .append(0, 0, 4)
        .pump(8)
        .append(0, 0, 5)
        .pump(8)
        .tick(60)
        .head_tick()
        .pump(4)
        // Device 1 is back, with the lost timeline in its pin: it is offered a head of the restored
        // timeline, higher than its own, before it is offered the record.
        .restart(2, false)
        .head_tick()
        .poll(0, 3)
        .tick(3)
        .deliver(12)
        .poll(2, 6)
        .quiesce()
        .build()
}

fn thousand_entries() -> Vec<Step> {
    let mut steps = Steps::new();
    for i in 0..1_100u32 {
        steps = steps
            .append((i % 2) as u8, 0, i as u16)
            .poll(1 + (i % 2) as u8, 1)
            .tick(3)
            .deliver(1)
            .polls(0, 6)
            .tick(3)
            .deliver(4)
            .poll(1, 4)
            .poll(2, 4);
    }
    steps.head_tick().quiesce().build()
}

fn restore_epoch_exhaustion() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .quiesce()
        .offsite()
        .append(0, 0, 2)
        .quiesce()
        .disaster(0, SigChoice::Map, true)
        .quiesce()
        .offsite()
        .disaster(0, SigChoice::Map, true)
        .quiesce()
        .build()
}

fn exhaustion_world() -> Config {
    small_world().with_start_epoch(u64::MAX - 1)
}

fn hostile_heads_and_entries() -> Vec<Step> {
    let mut steps = Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .append(0, 0, 3)
        .quiesce();
    for kind in AttackKind::ALL {
        steps = steps.attack(kind, 0, 0).attack(kind, 1, 0);
    }
    steps.quiesce().build()
}

fn leader_crash_recovers() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .quiesce()
        .append(0, 0, 3)
        .poll(1, 1)
        .tick(3)
        .deliver(1)
        .poll(0, 3)
        .crash(0)
        .restart(0, false)
        .quiesce()
        .build()
}

fn clock_jumps_do_not_run_stamps_backwards() -> Vec<Step> {
    Steps::new()
        .append(0, 0, 1)
        .quiesce()
        .jump(0, -3_600)
        .append(1, 0, 2)
        .quiesce()
        .jump(0, 7_200)
        .append(0, 0, 3)
        .quiesce()
        .skew(3, -500)
        .append(1, 0, 4)
        .quiesce()
        .build()
}

fn policy_steps() -> Vec<Step> {
    let mut steps = Steps::new();
    for i in 0..60u32 {
        steps = steps.policy(
            (i % 3 == 0) as u8,
            (i % 8) as u8,
            i.wrapping_mul(0x9E37_79B1),
        );
    }
    steps.build()
}

/// The directed scenarios that run green on the committed code.
///
/// Implements planning/architecture.md section 1.4 rule 2, section 2.17 and section 6.10 V3.
///
/// Caller: tests/s1_11, tests/s1_06, tests/s1_12, `mutants_table`.
pub const DIRECTED: &[Directed] = &[
    Directed {
        name: "restore_story",
        config: small_world,
        steps: restore_story,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "restore_then_hostile",
        config: small_world,
        steps: restore_then_hostile,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "offline_device_heals",
        config: small_world,
        steps: offline_device_heals,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "new_device_after_restore",
        config: small_world,
        steps: new_device_after_restore,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "checkpoints_by_age",
        config: small_world,
        steps: checkpoints_by_age,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "peers_and_withheld_heads",
        config: small_world,
        steps: peers_and_withheld_heads,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "hostile_heads_and_entries",
        config: small_world,
        steps: hostile_heads_and_entries,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "leader_crash_recovers",
        config: small_world,
        steps: leader_crash_recovers,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "clock_jumps_do_not_run_stamps_backwards",
        config: small_world,
        steps: clock_jumps_do_not_run_stamps_backwards,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "policy_steps",
        config: small_world,
        steps: policy_steps,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "restore_late_record_keeps_progress",
        config: small_world,
        steps: restore_late_record_keeps_progress,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "thousand_entries",
        config: small_world,
        steps: thousand_entries,
        expect: DirectedExpect::Green,
    },
    Directed {
        name: "restore_epoch_exhaustion",
        config: exhaustion_world,
        steps: restore_epoch_exhaustion,
        expect: DirectedExpect::Green,
    },
];

/// The directed scenario of this name.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `mutants_table::run_child`, tests.
pub fn directed_named(name: &str) -> Option<&'static Directed> {
    DIRECTED.iter().find(|d| d.name == name)
}

/// The directed scenarios that are kept as canary seed files, with the sentence of their note.
const CANARIES: [(&str, &str); 8] = [
    (
        "restore_story",
        "a record signed by the map key re-anchors every listed chain; held entries are sent again",
    ),
    (
        "restore_then_hostile",
        "hostile records, heads and entries before and after a restore are refused",
    ),
    (
        "offline_device_heals",
        "a device offline during the whole restore heals by fetching the record",
    ),
    (
        "new_device_after_restore",
        "a device born after the restore starts at the epoch",
    ),
    (
        "restore_late_record_keeps_progress",
        "a record that arrives after progress on the restored timeline keeps it",
    ),
    (
        "restore_epoch_exhaustion",
        "the counter of restores refuses the restore it cannot count",
    ),
    (
        "peers_and_withheld_heads",
        "a withheld head shows on the next meeting, a peer's junk is quiet",
    ),
    (
        "checkpoints_by_age",
        "a checkpoint is written after thirty days",
    ),
];

/// The canary seed files: the directed scenarios of the restore story, each as an `expect: pass` seed
/// (`tests/seeds/`; written by `simrun --write-canaries`, compared with the committed files by
/// `tests/s1_12`).
///
/// Implements planning/architecture.md section 1.4 rule 2 (a seed is a regression test) and section
/// 2.17 (the restore story).
///
/// Caller: src/bin/simrun.rs, tests/s1_12.
pub fn canary_files() -> Vec<SeedFile> {
    let mut files = Vec::new();
    for (name, note) in CANARIES {
        let Some(d) = directed_named(name) else {
            continue;
        };
        files.push(SeedFile {
            name: format!("canary-{}", name.replace('_', "-")),
            stage: Stage::S1,
            seed: DIRECTED_SEED,
            config: (d.config)(),
            invariant: InvariantId::RestoreHealsEveryDevice,
            expect: Expect::Pass,
            note: note.to_string(),
            steps: (d.steps)(),
        });
    }
    files
}

/// Runs a directed scenario.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `mutants_table::run_child`, tests/s1_11.
pub fn run_directed(d: &Directed) -> Option<Outcome> {
    let scenario = Scenario::new(DIRECTED_SEED, (d.config)()).ok()?;
    Some(scenario.replay(&(d.steps)()))
}

/// One crash case: the handler, the await point (the number of phases it had completed), and the
/// steps that bring the world there, crash the node, restart it and drain everything.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point).
///
/// Caller: tests/s1_04, tests/x04.
#[derive(Clone, Debug)]
pub struct CrashCase {
    /// The handler.
    pub kind: JobKind,
    /// The phases the job completed when the node crashed.
    pub phase: u8,
    /// The steps.
    pub steps: Vec<Step>,
}

/// The crash cases: one for each (handler, phase) of table 4.4, repeated three times in one list.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point).
///
/// Caller: tests/s1_04, tests/x04.
pub fn crash_cases() -> Vec<CrashCase> {
    let mut cases = Vec::new();
    for kind in JobKind::ALL {
        let phases = kind.phases();
        for phase in 1..=phases {
            let mut steps = Vec::new();
            for _ in 0..3 {
                steps.extend(crash_cycle(kind, phase));
            }
            cases.push(CrashCase { kind, phase, steps });
        }
    }
    cases
}

/// The steps of one crash cycle: reach `phase` of `kind`, crash the node, restart, drain.
fn crash_cycle(kind: JobKind, phase: u8) -> Vec<Step> {
    // A world that has one entry on every chain of the account and a copy off site.
    let base = Steps::new()
        .append(0, 0, 1)
        .append(1, 1, 2)
        .quiesce()
        .offsite()
        .quiesce();
    // `more` is how many phases are still to run after the first phase (which the trigger ran).
    let more = phase.saturating_sub(1);
    let (steps, node) = match kind {
        JobKind::LAppend => (
            base.append(0, 0, 7)
                .poll(1, 1)
                .tick(3)
                .deliver(1)
                .polls(0, more),
            0,
        ),
        JobKind::LHead => (base.head_tick().polls(0, more), 0),
        JobKind::LCheckpoint => (base.checkpoint(0).polls(0, more), 0),
        JobKind::LRestore => (base.disaster(0, SigChoice::Map, true).polls(0, more), 0),
        JobKind::LOffsite => (base.offsite(), 0),
        JobKind::LFetch => (base.fetch(0, 0).tick(3).deliver(1).polls(0, more), 0),
        JobKind::DOnHead => (
            base.head_tick()
                .poll(0, 3)
                .tick(3)
                .deliver(1)
                .polls(1, more),
            1,
        ),
        JobKind::DOnEntries => (
            base.append(0, 0, 7)
                .poll(1, 1)
                .tick(3)
                .deliver(1)
                .poll(0, 5)
                .tick(3)
                .deliver(2)
                .polls(1, more),
            1,
        ),
        JobKind::DOnRestore => (
            base.disaster(0, SigChoice::Map, true)
                .poll(0, 4)
                .tick(3)
                .deliver(1)
                .polls(1, more),
            1,
        ),
        JobKind::DPropose => {
            let steps = base.append(0, 0, 7);
            if phase <= 2 {
                (steps.polls(1, more), 1)
            } else {
                (
                    steps
                        .poll(1, 1)
                        .tick(3)
                        .deliver(1)
                        .poll(0, 5)
                        .tick(3)
                        .deliver(1)
                        .poll(1, phase.saturating_sub(2)),
                    1,
                )
            }
        }
        JobKind::DMeet => (base.meet(0, 1).polls(1, more), 1),
    };
    steps.crash(node).restart(node, false).quiesce().build()
}

/// What the entries of a step list name, for the tests that scan a list.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: tests/s1_04.
pub fn handler_names() -> Vec<&'static str> {
    HANDLERS.iter().map(|h| h.name).collect()
}
