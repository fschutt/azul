//! The configuration of a simulated world: stages, profiles, signature schemes, the fault rates
//! (each a [`Ppm`]: parts per million, checked at construction) and the limits of the harness
//! stated up front (planning/architecture.md section 1.4 rules 2 and 4, section 12 "the
//! deterministic simulation").
//!
//! A [`Config`] is a pure function of the seed ([`Config::standard`]): the profile cycles through
//! calm, chaos, restore-heavy and adversarial runs, and the size of the world (accounts, members,
//! devices, spaces) is read from other bits of the seed, so one number is the whole recipe of a
//! run.

use core::fmt;

use azlin_seams::sim::Rng;

use crate::draw::mix;

/// How many seeds the fast run draws at the highest implemented stage.
///
/// Implements planning/architecture.md section 12 (the deterministic simulation proves the logic).
///
/// Caller: tests/fast.rs, `RunOptions::fast`.
pub const FAST_SEEDS: u64 = 200;

/// How many steps a fast-run seed draws.
///
/// Implements planning/architecture.md section 12 (the deterministic simulation proves the logic).
///
/// Caller: tests/fast.rs, `RunOptions::fast`.
pub const FAST_STEPS: u32 = 250;

/// How many steps a long-run seed draws.
///
/// Implements planning/architecture.md section 12 (the deterministic simulation proves the logic).
///
/// Caller: tests/long.rs, src/bin/simrun.rs.
pub const LONG_STEPS: u32 = 2_000;

/// The most messages in flight in one network (rule 4: a send past it is refused and counted).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `net::SimNet`, `invariants` (the footprint ceilings).
pub const MAX_INFLIGHT: usize = 512;

/// The most jobs one node keeps (a message that finds the queue full is refused and counted).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `sched::Sched`, `invariants`.
pub const MAX_JOBS_PER_NODE: usize = 32;

/// The most open jobs (started, not finished) the interleaving tracker follows.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `sched::Sched`, `invariants`.
pub const MAX_OPEN_JOBS: usize = 64;

/// The most alarms one node keeps (a ring; the count is kept).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `device::Device`, `invariants`.
pub const MAX_ALARMS: usize = 256;

/// The trace lines kept in full (the rest is a rolling hash).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `world::Trace`, `Failure::trace_tail`.
pub const MAX_TRACE_TAIL: usize = 400;

/// The most records one simulated disk holds.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `disk::SimDisk`, `invariants`.
pub const MAX_DISK_RECORDS: usize = 8_192;

/// The most off-site copies the leader keeps (the oldest is dropped: a stated eviction of the model).
///
/// Implements planning/architecture.md section 2.17 (the off-site copy) and section 1.4 rule 4.
///
/// Caller: `leader::Leader`, `invariants`.
pub const MAX_OFFSITE_COPIES: usize = 8;

/// The most restores in one run (a `disaster` past it is a skip).
///
/// Implements planning/architecture.md section 2.17 (a restore re-anchors) and section 1.4 rule 4.
///
/// Caller: `world::World`, `invariants`.
pub const MAX_RESTORES: usize = 32;

/// The most rounds `quiesce` drains before it gives up (a typed harness failure, never a hang).
///
/// Implements planning/architecture.md section 1.4 rule 4 (every loop has a limit).
///
/// Caller: `world::World::quiesce`.
pub const QUIESCE_ROUNDS: usize = 500;

/// The most passes one drain of a `quiesce` round makes over the network and the jobs.
///
/// Implements planning/architecture.md section 1.4 rule 4 (every loop has a limit).
///
/// Caller: `world::World::drain`.
pub const DRAIN_ROUNDS: usize = 5_000;

/// The restore records the leader keeps and serves (glue rule G11).
///
/// Implements planning/architecture.md section 5.13 (devices heal from the announcement).
///
/// Caller: `leader::Leader`.
pub const MAX_KEPT_RECORDS: usize = 64;

/// Parts per million: a probability with a ceiling of 1,000,000. There is no `From<u32>`: a value
/// that is too large has no `Ppm`.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the simulator injects every fault).
///
/// Caller: `FaultRates`, `net::SimNet`, `disk::SimDisk`, tests/s1_01.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ppm(u32);

impl Ppm {
    /// Never.
    pub const ZERO: Ppm = Ppm(0);

    /// The ceiling: always.
    pub const MAX_PARTS: u32 = 1_000_000;

    /// `parts` per million; `None` above 1,000,000.
    pub const fn new(parts: u32) -> Option<Ppm> {
        if parts > Ppm::MAX_PARTS {
            None
        } else {
            Some(Ppm(parts))
        }
    }

    /// The parts per million.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// One draw: true with this probability. It always consumes one draw, so that changing a rate
    /// never moves the draws that come after it.
    pub fn hit(self, rng: &mut Rng) -> bool {
        rng.chance(self.0, Ppm::MAX_PARTS)
    }
}

/// A rate written as a constant: the argument is a literal that is at most 1,000,000.
const fn ppm(parts: u32) -> Ppm {
    if parts > Ppm::MAX_PARTS {
        Ppm(Ppm::MAX_PARTS)
    } else {
        Ppm(parts)
    }
}

/// The stages of the harness: each includes the earlier ones.
///
/// Implements planning/architecture.md section 12 (the deterministic simulation, grown in stages).
///
/// Caller: `Config`, `Scenario::standard`, `RunOptions`, every test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Chains, heads, restores and policy.
    S1,
    /// The team log.
    S2,
    /// Sync, enrollment, links, credentials.
    S3,
    /// The transparency log, the report ledger, entitlement replay.
    S4,
}

impl Stage {
    /// The highest stage this build implements.
    pub const HIGHEST: Stage = Stage::S1;

    /// Every stage, lowest first.
    pub const ALL: [Stage; 4] = [Stage::S1, Stage::S2, Stage::S3, Stage::S4];

    /// The number of the stage (1 to 4).
    pub const fn number(self) -> u8 {
        match self {
            Stage::S1 => 1,
            Stage::S2 => 2,
            Stage::S3 => 3,
            Stage::S4 => 4,
        }
    }

    /// The stage with this number.
    pub const fn from_number(number: u8) -> Option<Stage> {
        match number {
            1 => Some(Stage::S1),
            2 => Some(Stage::S2),
            3 => Some(Stage::S3),
            4 => Some(Stage::S4),
            _ => None,
        }
    }
}

/// The mix of faults and attacks of a run.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the simulator injects every fault).
///
/// Caller: `Config::standard`, `FaultRates::for_profile`, `scenario` (the step weights).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Profile {
    /// No fault but rare partitions and slow nodes.
    Calm,
    /// Every fault of the network, the clocks, the disks and the processes.
    Chaos,
    /// Chaos with restores drawn often.
    RestoreHeavy,
    /// Few faults, many attacks.
    Adversarial,
}

impl Profile {
    /// Every profile, in the order of `Config::standard`.
    pub const ALL: [Profile; 4] = [
        Profile::Calm,
        Profile::Chaos,
        Profile::RestoreHeavy,
        Profile::Adversarial,
    ];

    /// The word of the profile in a seed file.
    pub const fn name(self) -> &'static str {
        match self {
            Profile::Calm => "calm",
            Profile::Chaos => "chaos",
            Profile::RestoreHeavy => "restore-heavy",
            Profile::Adversarial => "adversarial",
        }
    }

    /// The profile of this word.
    pub fn from_name(name: &str) -> Option<Profile> {
        Profile::ALL.iter().copied().find(|p| p.name() == name)
    }
}

/// Which signature scheme signs and verifies in a run: the fast keyed hash of `azlin-seams`, or
/// real Ed25519 (a minority of seeds: the dev profile is slow).
///
/// Implements planning/architecture.md section 5.9 (every entry and head is signed).
///
/// Caller: `Config`, `keys::Keyring`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scheme {
    /// `azlin_seams::FakeScheme`.
    Fake,
    /// `azlin_keys` Ed25519 through `azlin_seams::Ed25519Verifier`.
    Ed25519,
}

impl Scheme {
    /// The word of the scheme in a seed file.
    pub const fn name(self) -> &'static str {
        match self {
            Scheme::Fake => "fake",
            Scheme::Ed25519 => "ed25519",
        }
    }

    /// The scheme of this word.
    pub fn from_name(name: &str) -> Option<Scheme> {
        match name {
            "fake" => Some(Scheme::Fake),
            "ed25519" => Some(Scheme::Ed25519),
            _ => None,
        }
    }
}

/// How the leader behaves: honest, or one of the bugs of a compromised or buggy leader (the way
/// the planted violations of the invariants are built). Off (`Honest`) in every generic profile.
///
/// Implements planning/architecture.md section 6.10 (V1, V3, S15) and section 5.9.
///
/// Caller: `Config`, `leader::Leader`, tests/planted_*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeaderVariant {
    /// The leader of the plan.
    Honest,
    /// Stamps with its raw clock: a jump back makes stamps run backwards.
    BuggyStamp,
    /// Writes new entries under the epoch before the restore.
    BuggyEpoch,
    /// Serves heads of an older state after a restore it never announced.
    SilentRollback,
    /// Acknowledges a proposal before it has fsynced it.
    AckBeforeFsync,
    /// Signs a second head at a position it already signed.
    DoubleSign,
    /// Never writes a checkpoint.
    NoCheckpoints,
}

impl LeaderVariant {
    /// Every variant.
    pub const ALL: [LeaderVariant; 7] = [
        LeaderVariant::Honest,
        LeaderVariant::BuggyStamp,
        LeaderVariant::BuggyEpoch,
        LeaderVariant::SilentRollback,
        LeaderVariant::AckBeforeFsync,
        LeaderVariant::DoubleSign,
        LeaderVariant::NoCheckpoints,
    ];

    /// The word of the variant in a seed file.
    pub const fn name(self) -> &'static str {
        match self {
            LeaderVariant::Honest => "honest",
            LeaderVariant::BuggyStamp => "buggy-stamp",
            LeaderVariant::BuggyEpoch => "buggy-epoch",
            LeaderVariant::SilentRollback => "silent-rollback",
            LeaderVariant::AckBeforeFsync => "ack-before-fsync",
            LeaderVariant::DoubleSign => "double-sign",
            LeaderVariant::NoCheckpoints => "no-checkpoints",
        }
    }

    /// The variant of this word.
    pub fn from_name(name: &str) -> Option<LeaderVariant> {
        LeaderVariant::ALL
            .iter()
            .copied()
            .find(|v| v.name() == name)
    }
}

/// A harness-side faulty double: the glue around the crates, made wrong on purpose, so that every
/// invariant has a planted violation (X12). It is never part of a seed file and never drawn.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a simulator that finds what a test of
/// one crate cannot).
///
/// Caller: `Scenario::with_defect`, tests/planted_*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Defect {
    /// No defect.
    None,
    /// The device forgets its pin before every head it is offered (a glue that rebuilds the pin).
    PinForgets,
    /// The head verifier accepts every signature.
    VerifierAcceptsAll,
    /// The device stores every entry it is sent without asking the chain.
    DeviceAdmitsUnchecked,
    /// The device follows a restore record signed by the group alone.
    GroupAloneTrust,
    /// The device does not raise the floor of its chain copies after a restore.
    NoChainFloor,
    /// The device rebuilds every listed chain at the survivor, the kept ones too.
    LateRecordRebuildsAll,
    /// The device leaves one listed chain where it was after a restore.
    ReanchorSkipsChain,
    /// The device drops the entries it held beyond the survivors instead of sending them again.
    ResendDropped,
    /// The device does nothing with a restore record.
    DeviceIgnoresRestore,
    /// `meet` drops the alarms of the peer comparison.
    MeetDropsAlarms,
    /// `meet` raises an alarm for a peer's junk.
    AlarmOnJunk,
    /// The recovery of a device replays its log out of order.
    RecoveryReorders,
    /// The glue classes a fork as an ordinary refusal.
    MisclassifiesFork,
    /// The network never refuses a send.
    UnboundedNet,
    /// The policy world applies a group's patch without the tightening check.
    PolicySkipsCheck,
    /// The policy world bumps the epoch on every patch.
    PolicyAlwaysBumps,
    /// The quorum count of the policy world counts devices.
    QuorumCountsDevices,
}

/// The rates of every fault, each per message, per step, per phase or per fsync as the table of
/// the brief says; every draw is per step (H2).
///
/// Implements planning/architecture.md section 1.4 rule 2 (network, disk, clock and process
/// faults).
///
/// Caller: `Config`, `net::SimNet::send`, `disk::SimDisk`, `scenario` (the step weights).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaultRates {
    /// The delay of a message in seconds, inclusive.
    pub delay_secs: (u16, u16),
    /// A message is lost.
    pub loss: Ppm,
    /// A message is delivered twice.
    pub duplication: Ppm,
    /// A message gets extra delay, so it can pass a later one.
    pub reordering: Ppm,
    /// A message is corrupted in flight.
    pub corruption: Ppm,
    /// A step draws a partition.
    pub partition: Ppm,
    /// A step draws a slow node.
    pub slow: Ppm,
    /// A step draws a clock skew or jump.
    pub clock: Ppm,
    /// A node crashes at a phase.
    pub crash: Ppm,
    /// A crash tears the last unsynced record.
    pub torn: Ppm,
    /// An fsync lies.
    pub lost_fsync: Ppm,
    /// A step draws a read error.
    pub read_error: Ppm,
    /// A step draws a bit flip.
    pub bit_flip: Ppm,
    /// A step draws a vanishing disk.
    pub vanish: Ppm,
}

impl FaultRates {
    /// No fault at all and a delay of one second.
    pub const fn none() -> FaultRates {
        FaultRates {
            delay_secs: (1, 1),
            loss: Ppm::ZERO,
            duplication: Ppm::ZERO,
            reordering: Ppm::ZERO,
            corruption: Ppm::ZERO,
            partition: Ppm::ZERO,
            slow: Ppm::ZERO,
            clock: Ppm::ZERO,
            crash: Ppm::ZERO,
            torn: Ppm::ZERO,
            lost_fsync: Ppm::ZERO,
            read_error: Ppm::ZERO,
            bit_flip: Ppm::ZERO,
            vanish: Ppm::ZERO,
        }
    }

    /// The provisional rates of a profile (the table of the brief, section 4.3; estimates).
    pub const fn for_profile(profile: Profile) -> FaultRates {
        match profile {
            Profile::Calm => FaultRates {
                delay_secs: (0, 2),
                partition: ppm(2_000),
                slow: ppm(2_000),
                ..FaultRates::none()
            },
            Profile::Chaos => FaultRates {
                delay_secs: (0, 30),
                loss: ppm(30_000),
                duplication: ppm(20_000),
                reordering: ppm(100_000),
                corruption: ppm(5_000),
                partition: ppm(25_000),
                slow: ppm(25_000),
                clock: ppm(20_000),
                crash: ppm(10_000),
                torn: ppm(100_000),
                lost_fsync: ppm(10_000),
                read_error: ppm(3_000),
                bit_flip: ppm(3_000),
                vanish: ppm(1_500),
            },
            Profile::RestoreHeavy => FaultRates {
                delay_secs: (0, 30),
                loss: ppm(30_000),
                duplication: ppm(20_000),
                reordering: ppm(100_000),
                corruption: ppm(5_000),
                partition: ppm(25_000),
                slow: ppm(25_000),
                clock: ppm(20_000),
                crash: ppm(20_000),
                torn: ppm(100_000),
                lost_fsync: ppm(50_000),
                read_error: ppm(3_000),
                bit_flip: ppm(3_000),
                vanish: ppm(1_500),
            },
            Profile::Adversarial => FaultRates {
                delay_secs: (0, 5),
                partition: ppm(2_000),
                slow: ppm(2_000),
                crash: ppm(5_000),
                ..FaultRates::none()
            },
        }
    }
}

/// Why a [`Config`] is refused.
///
/// Implements planning/architecture.md section 1.4 rule 4 (limits stated up front).
///
/// Caller: `Config::validate`, `Scenario::new`, `SeedFile::parse`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The number of accounts is outside 1..=2.
    Accounts(u8),
    /// The number of members per account is outside the stage's range.
    Members(u8),
    /// The number of devices per member is outside the stage's range.
    Devices(u8),
    /// The number of spaces per account is outside 0..=3.
    Spaces(u8),
    /// The number of steps is outside 1..=1,000,000.
    Steps(u32),
    /// A delay range whose minimum is above its maximum.
    DelayRange,
}

error_text!(ConfigError {
    ConfigError::Accounts(n) => "accounts must be 1 to 2, not {n}",
    ConfigError::Members(n) => "members per account must be 2 to 3 at stage 1, not {n}",
    ConfigError::Devices(n) => "devices per member must be 1 to 2 at stage 1, not {n}",
    ConfigError::Spaces(n) => "spaces per account must be 0 to 3, not {n}",
    ConfigError::Steps(n) => "steps must be 1 to 1000000, not {n}",
    ConfigError::DelayRange => "the delay range is empty",
});

/// One run's configuration: the stage, the profile, the signature scheme, the size of the world,
/// the number of steps and the fault rates. A pure function of the seed ([`Config::standard`]).
///
/// Implements planning/architecture.md section 1.4 rules 2 and 4 (faults and limits as parameters).
///
/// Caller: `Scenario`, `SeedFile`, every test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The stage the run is at.
    pub stage: Stage,
    /// The mix of faults and attacks.
    pub profile: Profile,
    /// The signature scheme.
    pub scheme: Scheme,
    /// The number of accounts.
    pub accounts: u8,
    /// The number of members of each account.
    pub members: u8,
    /// The number of devices of each member.
    pub devices: u8,
    /// The number of spaces of each account.
    pub spaces: u8,
    /// The number of steps drawn (the automatic `quiesce` comes on top).
    pub steps: u32,
    /// The rates of the faults.
    pub faults: FaultRates,
    /// The behaviour of the leader (honest in every generic run).
    pub leader: LeaderVariant,
    /// The restore epoch the world starts in (0 unless a scenario tests the exhaustion of the
    /// counter).
    pub start_epoch: u64,
}

/// The share of seeds that use real Ed25519 unless the run says otherwise.
///
/// Implements planning/architecture.md section 5.9 (signatures are real in production).
///
/// Caller: `Config::standard`, `RunOptions`.
pub const DEFAULT_REAL_PERMILLE: u16 = 50;

impl Config {
    /// The configuration of a seed: the profile is `seed % 4`, the size of the world is read from
    /// the bits above, the scheme is real Ed25519 for one seed in twenty.
    pub fn standard(seed: u64, stage: Stage) -> Config {
        Config::standard_with(seed, stage, DEFAULT_REAL_PERMILLE)
    }

    /// [`Config::standard`] with another share of real-signature seeds (parts per thousand).
    pub fn standard_with(seed: u64, stage: Stage, real_permille: u16) -> Config {
        let profile = match seed % 4 {
            0 => Profile::Calm,
            1 => Profile::Chaos,
            2 => Profile::RestoreHeavy,
            _ => Profile::Adversarial,
        };
        let scheme = if mix(seed, 0x5CE_4E) % 1000 < u64::from(real_permille) {
            Scheme::Ed25519
        } else {
            Scheme::Fake
        };
        Config {
            stage,
            profile,
            scheme,
            accounts: 1 + ((seed >> 2) % 2) as u8,
            members: 2 + ((seed >> 4) % 2) as u8,
            devices: 1 + ((seed >> 6) % 2) as u8,
            spaces: ((seed >> 8) % 3) as u8,
            steps: FAST_STEPS,
            faults: FaultRates::for_profile(profile),
            leader: LeaderVariant::Honest,
            start_epoch: 0,
        }
    }

    /// The same configuration with another number of steps.
    #[must_use]
    pub fn with_steps(mut self, steps: u32) -> Config {
        self.steps = steps;
        self
    }

    /// The same configuration with the fault rates of another profile.
    #[must_use]
    pub fn with_profile(mut self, profile: Profile) -> Config {
        self.profile = profile;
        self.faults = FaultRates::for_profile(profile);
        self
    }

    /// The same configuration with another signature scheme.
    #[must_use]
    pub fn with_scheme(mut self, scheme: Scheme) -> Config {
        self.scheme = scheme;
        self
    }

    /// The same configuration with another size of world.
    #[must_use]
    pub fn with_size(mut self, accounts: u8, members: u8, devices: u8, spaces: u8) -> Config {
        self.accounts = accounts;
        self.members = members;
        self.devices = devices;
        self.spaces = spaces;
        self
    }

    /// The same configuration starting in another restore epoch.
    #[must_use]
    pub fn with_start_epoch(mut self, start_epoch: u64) -> Config {
        self.start_epoch = start_epoch;
        self
    }

    /// The same configuration with another leader.
    #[must_use]
    pub fn with_leader(mut self, leader: LeaderVariant) -> Config {
        self.leader = leader;
        self
    }

    /// Whether the numbers are inside the limits of the stage.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=2).contains(&self.accounts) {
            return Err(ConfigError::Accounts(self.accounts));
        }
        if !(2..=3).contains(&self.members) {
            return Err(ConfigError::Members(self.members));
        }
        if !(1..=2).contains(&self.devices) {
            return Err(ConfigError::Devices(self.devices));
        }
        if self.spaces > 3 {
            return Err(ConfigError::Spaces(self.spaces));
        }
        if !(1..=1_000_000).contains(&self.steps) {
            return Err(ConfigError::Steps(self.steps));
        }
        if self.faults.delay_secs.0 > self.faults.delay_secs.1 {
            return Err(ConfigError::DelayRange);
        }
        Ok(())
    }

    /// The number of devices in the world.
    pub fn device_count(&self) -> usize {
        usize::from(self.accounts) * usize::from(self.members) * usize::from(self.devices)
    }

    /// The number of chains: one team log and `spaces` space chains for each account.
    pub fn chain_count(&self) -> usize {
        usize::from(self.accounts) * (1 + usize::from(self.spaces))
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "profile={} scheme={} accounts={} members={} devices={} spaces={} steps={}",
            self.profile.name(),
            self.scheme.name(),
            self.accounts,
            self.members,
            self.devices,
            self.spaces,
            self.steps
        )?;
        if self.leader != LeaderVariant::Honest {
            write!(f, " leader={}", self.leader.name())?;
        }
        if self.start_epoch != 0 {
            write!(f, " start_epoch={}", self.start_epoch)?;
        }
        Ok(())
    }
}
