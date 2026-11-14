//! The steps of a scenario: plain values (small integers and enums, no references to world
//! objects) with a strict one-line text form (planning/architecture.md section 1.4 rule 2: a
//! failing seed replays exactly; section 12: the deterministic simulation).
//!
//! A step names its targets by INDEX and the interpreter takes the index modulo the current
//! population; a step that does not apply is a skip, so removing a step while shrinking never
//! makes another step invalid. Every step carries a `salt`: every random decision the step causes
//! draws from `Rng::new(mix(seed, salt))` (rule H2).
//!
//! The text form is one line, `verb key=value ... salt=0x........`: the keys of a verb in their
//! exact order, single spaces, no trailing text. An unsigned number is `0x` and lower-case hex of
//! a fixed width (2, 4 or 8 digits for a `u8`, `u16`, `u32`), a signed number is plain decimal, a
//! flag is `0` or `1`, a word is one of the words of its key. Anything else is a typed
//! [`StepParseError`].

use azlin_seams::sim::Rng;

use crate::draw::below;
use crate::time::Shift;

/// Which keys sign a restore record (the `sig` key of `disaster`).
///
/// Implements planning/architecture.md section 12 E4 (a restore is signed by the map key) and
/// section 2.17.
///
/// Caller: `Step::Disaster`, `leader`, `directed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SigChoice {
    /// The map key (the operator's).
    Map,
    /// The group key alone.
    Group,
    /// Both.
    Both,
    /// Nobody.
    None,
}

impl SigChoice {
    /// Every choice, in the order of the words.
    pub const ALL: [SigChoice; 4] = [
        SigChoice::Map,
        SigChoice::Group,
        SigChoice::Both,
        SigChoice::None,
    ];

    /// The word of the choice in a step line.
    pub const fn name(self) -> &'static str {
        match self {
            SigChoice::Map => "map",
            SigChoice::Group => "group",
            SigChoice::Both => "both",
            SigChoice::None => "none",
        }
    }
}

/// Which disk fault a `disk` step arms.
///
/// Implements planning/architecture.md section 1.4 rule 2 (disk faults: a flush that lies, torn
/// writes, sector errors, flipped bits, vanishing disks).
///
/// Caller: `Step::Disk`, `world::World::step`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiskFault {
    /// The next fsyncs lie.
    Lie,
    /// The next crash tears the last record.
    Tear,
    /// The next read fails.
    ReadErr,
    /// A record turns corrupt.
    Flip,
    /// Everything vanishes.
    Vanish,
}

impl DiskFault {
    /// Every fault, in the order of the words.
    pub const ALL: [DiskFault; 5] = [
        DiskFault::Lie,
        DiskFault::Tear,
        DiskFault::ReadErr,
        DiskFault::Flip,
        DiskFault::Vanish,
    ];

    /// The word of the fault in a step line.
    pub const fn name(self) -> &'static str {
        match self {
            DiskFault::Lie => "lie",
            DiskFault::Tear => "tear",
            DiskFault::ReadErr => "readerr",
            DiskFault::Flip => "flip",
            DiskFault::Vanish => "vanish",
        }
    }
}

/// The moves of the adversary (the server, the database and the network), and the verdict class
/// each must meet (section 4.5 of the brief; `adversary::expected`).
///
/// Implements planning/architecture.md section 6.10 S15 (a malicious server), section 5.9 (a fork
/// is refused and alarmed) and section 12 E4.
///
/// Caller: `Step::Attack`, `adversary`, `invariants`, `coverage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttackKind {
    /// A validly signed head older than the device's pin.
    ReplayOldHead,
    /// A validly signed head of an epoch below the device's floor.
    LostTimelineHead,
    /// An old signed head shown to one device only.
    StaleHeadToOne,
    /// The group key signs a second head at a held position.
    ForkHead,
    /// A second entry at a held position, validly chained.
    ForkEntry,
    /// An entry whose copy does not match its hash.
    CorruptEntry,
    /// A head with a flipped signature byte.
    CorruptHead,
    /// A head with no signature.
    UnsignedHead,
    /// A head of another chain.
    WrongChainHead,
    /// An entry of another chain.
    WrongChainEntry,
    /// A genesis that differs from the one the chain holds.
    SecondGenesis,
    /// An entry that names another predecessor.
    PrevSwap,
    /// An entry beyond the next position.
    GapEntry,
    /// An entry whose stamp runs backwards.
    StampBackwardsEntry,
    /// An entry of an older restore epoch.
    OldEpochEntry,
    /// A restore record already applied.
    ReplayOldRestore,
    /// A restore record signed by the group key only.
    GroupAloneRestore,
    /// A restore record signed by nobody.
    RestoreWithoutSignature,
    /// A peer sends a head with a bad signature.
    PeerJunk,
    /// A peer sends a validly signed head that forks the receiver's.
    PeerFork,
}

impl AttackKind {
    /// Every kind, in declaration order (the index of a kind is its position here).
    pub const ALL: [AttackKind; 20] = [
        AttackKind::ReplayOldHead,
        AttackKind::LostTimelineHead,
        AttackKind::StaleHeadToOne,
        AttackKind::ForkHead,
        AttackKind::ForkEntry,
        AttackKind::CorruptEntry,
        AttackKind::CorruptHead,
        AttackKind::UnsignedHead,
        AttackKind::WrongChainHead,
        AttackKind::WrongChainEntry,
        AttackKind::SecondGenesis,
        AttackKind::PrevSwap,
        AttackKind::GapEntry,
        AttackKind::StampBackwardsEntry,
        AttackKind::OldEpochEntry,
        AttackKind::ReplayOldRestore,
        AttackKind::GroupAloneRestore,
        AttackKind::RestoreWithoutSignature,
        AttackKind::PeerJunk,
        AttackKind::PeerFork,
    ];

    /// The word of the kind in a step line.
    pub const fn name(self) -> &'static str {
        match self {
            AttackKind::ReplayOldHead => "replay-old-head",
            AttackKind::LostTimelineHead => "lost-timeline-head",
            AttackKind::StaleHeadToOne => "stale-head-to-one",
            AttackKind::ForkHead => "fork-head",
            AttackKind::ForkEntry => "fork-entry",
            AttackKind::CorruptEntry => "corrupt-entry",
            AttackKind::CorruptHead => "corrupt-head",
            AttackKind::UnsignedHead => "unsigned-head",
            AttackKind::WrongChainHead => "wrong-chain-head",
            AttackKind::WrongChainEntry => "wrong-chain-entry",
            AttackKind::SecondGenesis => "second-genesis",
            AttackKind::PrevSwap => "prev-swap",
            AttackKind::GapEntry => "gap-entry",
            AttackKind::StampBackwardsEntry => "stamp-backwards-entry",
            AttackKind::OldEpochEntry => "old-epoch-entry",
            AttackKind::ReplayOldRestore => "replay-old-restore",
            AttackKind::GroupAloneRestore => "group-alone-restore",
            AttackKind::RestoreWithoutSignature => "restore-without-signature",
            AttackKind::PeerJunk => "peer-junk",
            AttackKind::PeerFork => "peer-fork",
        }
    }

    /// The kind of this word.
    pub fn from_name(name: &str) -> Option<AttackKind> {
        AttackKind::ALL.iter().copied().find(|k| k.name() == name)
    }
}

/// The words a key may take, by key type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WordSet {
    Sig,
    Disk,
    Keep,
    Attack,
}

const SIG_WORDS: [&str; 4] = ["map", "group", "both", "none"];
const DISK_WORDS: [&str; 5] = ["lie", "tear", "readerr", "flip", "vanish"];
const KEEP_WORDS: [&str; 2] = ["keep", "wipe"];
const ATTACK_WORDS: [&str; 20] = [
    "replay-old-head",
    "lost-timeline-head",
    "stale-head-to-one",
    "fork-head",
    "fork-entry",
    "corrupt-entry",
    "corrupt-head",
    "unsigned-head",
    "wrong-chain-head",
    "wrong-chain-entry",
    "second-genesis",
    "prev-swap",
    "gap-entry",
    "stamp-backwards-entry",
    "old-epoch-entry",
    "replay-old-restore",
    "group-alone-restore",
    "restore-without-signature",
    "peer-junk",
    "peer-fork",
];

impl WordSet {
    fn words(self) -> &'static [&'static str] {
        match self {
            WordSet::Sig => &SIG_WORDS,
            WordSet::Disk => &DISK_WORDS,
            WordSet::Keep => &KEEP_WORDS,
            WordSet::Attack => &ATTACK_WORDS,
        }
    }
}

/// The type of the value of a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    U8,
    U16,
    U32,
    I32,
    Flag,
    Word(WordSet),
}

/// A parsed or to-be-written value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Val {
    U(u32),
    S(i32),
    W(usize),
}

/// The one-word names of the verbs, one per variant of [`Step`].
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
///
/// Caller: `Step::kind`, `coverage`, `scenario` (the step weights), tests/x02.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StepKind {
    /// `append`.
    Append,
    /// `deliver`.
    Deliver,
    /// `poll`.
    Poll,
    /// `tick`.
    Tick,
    /// `head_tick`.
    HeadTick,
    /// `fetch`.
    Fetch,
    /// `meet`.
    Meet,
    /// `checkpoint`.
    Checkpoint,
    /// `offsite`.
    Offsite,
    /// `disaster`.
    Disaster,
    /// `resend`.
    Resend,
    /// `partition`.
    Partition,
    /// `heal`.
    Heal,
    /// `slow`.
    Slow,
    /// `skew`.
    Skew,
    /// `jump`.
    Jump,
    /// `crash`.
    Crash,
    /// `restart`.
    Restart,
    /// `disk`.
    Disk,
    /// `attack`.
    Attack,
    /// `policy`.
    Policy,
    /// `quiesce`.
    Quiesce,
}

impl StepKind {
    /// Every verb.
    pub const ALL: [StepKind; 22] = [
        StepKind::Append,
        StepKind::Deliver,
        StepKind::Poll,
        StepKind::Tick,
        StepKind::HeadTick,
        StepKind::Fetch,
        StepKind::Meet,
        StepKind::Checkpoint,
        StepKind::Offsite,
        StepKind::Disaster,
        StepKind::Resend,
        StepKind::Partition,
        StepKind::Heal,
        StepKind::Slow,
        StepKind::Skew,
        StepKind::Jump,
        StepKind::Crash,
        StepKind::Restart,
        StepKind::Disk,
        StepKind::Attack,
        StepKind::Policy,
        StepKind::Quiesce,
    ];

    /// The verb as written in a step line.
    pub const fn verb(self) -> &'static str {
        match self {
            StepKind::Append => "append",
            StepKind::Deliver => "deliver",
            StepKind::Poll => "poll",
            StepKind::Tick => "tick",
            StepKind::HeadTick => "head_tick",
            StepKind::Fetch => "fetch",
            StepKind::Meet => "meet",
            StepKind::Checkpoint => "checkpoint",
            StepKind::Offsite => "offsite",
            StepKind::Disaster => "disaster",
            StepKind::Resend => "resend",
            StepKind::Partition => "partition",
            StepKind::Heal => "heal",
            StepKind::Slow => "slow",
            StepKind::Skew => "skew",
            StepKind::Jump => "jump",
            StepKind::Crash => "crash",
            StepKind::Restart => "restart",
            StepKind::Disk => "disk",
            StepKind::Attack => "attack",
            StepKind::Policy => "policy",
            StepKind::Quiesce => "quiesce",
        }
    }

    /// The keys of the verb (without `salt`), in their exact order.
    fn keys(self) -> &'static [(&'static str, Ty)] {
        match self {
            StepKind::Append => &[("dev", Ty::U8), ("chain", Ty::U8), ("payload", Ty::U16)],
            StepKind::Deliver => &[("n", Ty::U8)],
            StepKind::Poll => &[("node", Ty::U8), ("n", Ty::U8)],
            StepKind::Tick => &[("secs", Ty::U32)],
            StepKind::HeadTick => &[],
            StepKind::Fetch => &[("dev", Ty::U8), ("chain", Ty::U8)],
            StepKind::Meet => &[("a", Ty::U8), ("b", Ty::U8)],
            StepKind::Checkpoint => &[("chain", Ty::U8)],
            StepKind::Offsite => &[],
            StepKind::Disaster => &[
                ("copy", Ty::U8),
                ("sig", Ty::Word(WordSet::Sig)),
                ("announce", Ty::Flag),
            ],
            StepKind::Resend => &[("dev", Ty::U8)],
            StepKind::Partition => &[("mask", Ty::U16)],
            StepKind::Heal => &[("mask", Ty::U16)],
            StepKind::Slow => &[("node", Ty::U8), ("factor", Ty::U8)],
            StepKind::Skew => &[("node", Ty::U8), ("secs", Ty::I32)],
            StepKind::Jump => &[("node", Ty::U8), ("secs", Ty::I32)],
            StepKind::Crash => &[("node", Ty::U8)],
            StepKind::Restart => &[("node", Ty::U8), ("disk", Ty::Word(WordSet::Keep))],
            StepKind::Disk => &[("node", Ty::U8), ("fault", Ty::Word(WordSet::Disk))],
            StepKind::Attack => &[
                ("kind", Ty::Word(WordSet::Attack)),
                ("dev", Ty::U8),
                ("chain", Ty::U8),
            ],
            StepKind::Policy => &[("scope", Ty::U8), ("group", Ty::U8), ("patch", Ty::U32)],
            StepKind::Quiesce => &[],
        }
    }

    /// The verb of this word.
    pub fn from_verb(verb: &str) -> Option<StepKind> {
        StepKind::ALL.iter().copied().find(|k| k.verb() == verb)
    }
}

/// One step of a scenario. Every variant ends with the `salt` of the step.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the simulator injects every fault and
/// a failing seed replays exactly) and section 12 (the deterministic simulation).
///
/// Caller: `Scenario::run`, `Scenario::replay`, `world::World::step`, `SeedFile`, tests/x02.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    /// A device proposes an entry on a chain, built on its own view of the tip.
    Append {
        /// The device.
        dev: u8,
        /// The chain of the device's account.
        chain: u8,
        /// The payload.
        payload: u16,
        /// The salt.
        salt: u32,
    },
    /// Deliver the earliest due messages.
    Deliver {
        /// How many.
        n: u8,
        /// The salt.
        salt: u32,
    },
    /// Run phases of the jobs of one node.
    Poll {
        /// The node.
        node: u8,
        /// How many phases.
        n: u8,
        /// The salt.
        salt: u32,
    },
    /// Advance every clock.
    Tick {
        /// How many seconds.
        secs: u32,
        /// The salt.
        salt: u32,
    },
    /// The leader signs and sends the heads of all chains.
    HeadTick {
        /// The salt.
        salt: u32,
    },
    /// A device asks the leader for entries, the head and restore records.
    Fetch {
        /// The device.
        dev: u8,
        /// The chain of the device's account.
        chain: u8,
        /// The salt.
        salt: u32,
    },
    /// Two devices exchange their signed heads.
    Meet {
        /// The first device.
        a: u8,
        /// The second device.
        b: u8,
        /// The salt.
        salt: u32,
    },
    /// The leader records a checkpoint now.
    Checkpoint {
        /// The chain.
        chain: u8,
        /// The salt.
        salt: u32,
    },
    /// The leader takes an off-site copy.
    Offsite {
        /// The salt.
        salt: u32,
    },
    /// The operator restores the leader from an off-site copy.
    Disaster {
        /// Which copy.
        copy: u8,
        /// Who signs the record.
        sig: SigChoice,
        /// Whether the record is broadcast (`false` is a silent rollback).
        announce: bool,
        /// The salt.
        salt: u32,
    },
    /// A device sends again what it holds beyond the survivors.
    Resend {
        /// The device.
        dev: u8,
        /// The salt.
        salt: u32,
    },
    /// Cut a group of nodes off.
    Partition {
        /// The nodes (a bit mask).
        mask: u16,
        /// The salt.
        salt: u32,
    },
    /// Join a group of nodes again.
    Heal {
        /// The nodes (a bit mask).
        mask: u16,
        /// The salt.
        salt: u32,
    },
    /// Make one node slow.
    Slow {
        /// The node.
        node: u8,
        /// The factor of its delays.
        factor: u8,
        /// The salt.
        salt: u32,
    },
    /// Set a clock offset.
    Skew {
        /// The clock (a node, or one of the two voters).
        node: u8,
        /// The offset.
        shift: Shift,
        /// The salt.
        salt: u32,
    },
    /// Jump a clock.
    Jump {
        /// The clock (a node, or one of the two voters).
        node: u8,
        /// The jump.
        shift: Shift,
        /// The salt.
        salt: u32,
    },
    /// A process crash at the current await point.
    Crash {
        /// The node.
        node: u8,
        /// The salt.
        salt: u32,
    },
    /// A restart with or without the disk.
    Restart {
        /// The node.
        node: u8,
        /// Whether the disk is wiped.
        wipe: bool,
        /// The salt.
        salt: u32,
    },
    /// Arm a disk fault.
    Disk {
        /// The node.
        node: u8,
        /// The fault.
        fault: DiskFault,
        /// The salt.
        salt: u32,
    },
    /// The adversary acts.
    Attack {
        /// The move.
        kind: AttackKind,
        /// The device it targets.
        dev: u8,
        /// The chain of the device's account.
        chain: u8,
        /// The salt.
        salt: u32,
    },
    /// A policy change in the policy world.
    Policy {
        /// Whether the account's own policy or a group's (0 is the account).
        scope: u8,
        /// The group.
        group: u8,
        /// The seed of the random patch.
        patch: u32,
        /// The salt.
        salt: u32,
    },
    /// Heal everything, drain everything, then run the convergence checks.
    Quiesce {
        /// The salt.
        salt: u32,
    },
}

/// Why a step line is not a step. One variant for each way to be wrong.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly: the
/// text of a step is strict).
///
/// Caller: `Step::parse`, `SeedFile::parse`, tests/x02.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepParseError {
    /// The line is empty.
    Empty,
    /// The line starts or ends with a space.
    EdgeSpace,
    /// Two spaces in a row.
    DoubleSpace,
    /// The verb is not one of the verbs.
    UnknownVerb,
    /// A key is missing (the line ended early).
    MissingKey,
    /// The `salt` key is missing.
    MissingSalt,
    /// A token is not `key=value`.
    NotKeyValue,
    /// A key that no verb has, or that this verb does not have.
    UnknownKey,
    /// A key of this verb, but not the one that comes next.
    KeyOutOfOrder,
    /// A key that was given before.
    RepeatedKey,
    /// A value that is not a number of the form of its key.
    BadNumber,
    /// A hex number with the wrong number of digits.
    BadWidth,
    /// A hex number with an upper-case digit.
    UpperCaseHex,
    /// A negative number where the key is unsigned.
    NegativeUnsigned,
    /// A number outside the range of its key.
    OutOfRange,
    /// A word that is not one of the words of its key.
    BadWord,
    /// Text after the salt.
    TrailingText,
}

error_text!(StepParseError {
    StepParseError::Empty => "the line is empty",
    StepParseError::EdgeSpace => "the line starts or ends with a space",
    StepParseError::DoubleSpace => "two spaces in a row",
    StepParseError::UnknownVerb => "unknown verb",
    StepParseError::MissingKey => "a key is missing",
    StepParseError::MissingSalt => "the salt is missing",
    StepParseError::NotKeyValue => "a token is not key=value",
    StepParseError::UnknownKey => "unknown key",
    StepParseError::KeyOutOfOrder => "keys out of order",
    StepParseError::RepeatedKey => "a key is repeated",
    StepParseError::BadNumber => "not a number of the form of its key",
    StepParseError::BadWidth => "the wrong number of hex digits",
    StepParseError::UpperCaseHex => "upper-case hex digit",
    StepParseError::NegativeUnsigned => "a negative number where the key is unsigned",
    StepParseError::OutOfRange => "number out of range",
    StepParseError::BadWord => "not a word of this key",
    StepParseError::TrailingText => "trailing text",
});

/// A cursor over parsed values, so that building a step never indexes.
struct Vals<'a> {
    vals: &'a [Val],
    at: usize,
}

impl Vals<'_> {
    fn take(&mut self) -> Result<Val, StepParseError> {
        let val = self.vals.get(self.at).copied();
        self.at = self.at.saturating_add(1);
        val.ok_or(StepParseError::MissingKey)
    }

    fn u8(&mut self) -> Result<u8, StepParseError> {
        match self.take()? {
            Val::U(v) => u8::try_from(v).map_err(|_| StepParseError::OutOfRange),
            Val::S(_) | Val::W(_) => Err(StepParseError::BadNumber),
        }
    }

    fn u16(&mut self) -> Result<u16, StepParseError> {
        match self.take()? {
            Val::U(v) => u16::try_from(v).map_err(|_| StepParseError::OutOfRange),
            Val::S(_) | Val::W(_) => Err(StepParseError::BadNumber),
        }
    }

    fn u32(&mut self) -> Result<u32, StepParseError> {
        match self.take()? {
            Val::U(v) => Ok(v),
            Val::S(_) | Val::W(_) => Err(StepParseError::BadNumber),
        }
    }

    fn i32(&mut self) -> Result<i32, StepParseError> {
        match self.take()? {
            Val::S(v) => Ok(v),
            Val::U(_) | Val::W(_) => Err(StepParseError::BadNumber),
        }
    }

    fn flag(&mut self) -> Result<bool, StepParseError> {
        match self.take()? {
            Val::U(0) => Ok(false),
            Val::U(1) => Ok(true),
            Val::U(_) => Err(StepParseError::OutOfRange),
            Val::S(_) | Val::W(_) => Err(StepParseError::BadNumber),
        }
    }

    fn word(&mut self) -> Result<usize, StepParseError> {
        match self.take()? {
            Val::W(v) => Ok(v),
            Val::U(_) | Val::S(_) => Err(StepParseError::BadWord),
        }
    }
}

fn parse_hex(text: &str, width: usize) -> Result<u32, StepParseError> {
    if text.starts_with('-') {
        return Err(StepParseError::NegativeUnsigned);
    }
    let digits = text.strip_prefix("0x").ok_or(StepParseError::BadNumber)?;
    if digits.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err(StepParseError::UpperCaseHex);
    }
    if digits.len() != width {
        return Err(StepParseError::BadWidth);
    }
    if !digits
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StepParseError::BadNumber);
    }
    u32::from_str_radix(digits, 16).map_err(|_| StepParseError::BadNumber)
}

fn parse_signed(text: &str) -> Result<i32, StepParseError> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(StepParseError::BadNumber);
    }
    if (digits.len() > 1 && digits.starts_with('0')) || text == "-0" {
        return Err(StepParseError::BadNumber);
    }
    text.parse::<i32>().map_err(|_| StepParseError::OutOfRange)
}

fn parse_value(ty: Ty, text: &str) -> Result<Val, StepParseError> {
    match ty {
        Ty::U8 => parse_hex(text, 2).map(Val::U),
        Ty::U16 => parse_hex(text, 4).map(Val::U),
        Ty::U32 => parse_hex(text, 8).map(Val::U),
        Ty::I32 => parse_signed(text).map(Val::S),
        Ty::Flag => match text {
            "0" => Ok(Val::U(0)),
            "1" => Ok(Val::U(1)),
            _ if text.starts_with('-') => Err(StepParseError::NegativeUnsigned),
            _ if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                Err(StepParseError::OutOfRange)
            }
            _ => Err(StepParseError::BadNumber),
        },
        Ty::Word(set) => set
            .words()
            .iter()
            .position(|w| *w == text)
            .map(Val::W)
            .ok_or(StepParseError::BadWord),
    }
}

fn write_value(ty: Ty, val: Val, out: &mut String) {
    match (ty, val) {
        (Ty::U8, Val::U(v)) => out.push_str(&format!("0x{v:02x}")),
        (Ty::U16, Val::U(v)) => out.push_str(&format!("0x{v:04x}")),
        (Ty::U32, Val::U(v)) => out.push_str(&format!("0x{v:08x}")),
        (Ty::I32, Val::S(v)) => out.push_str(&v.to_string()),
        (Ty::Flag, Val::U(v)) => out.push(if v == 0 { '0' } else { '1' }),
        (Ty::Word(set), Val::W(i)) => {
            if let Some(word) = set.words().get(i) {
                out.push_str(word);
            }
        }
        // A value of the wrong form for its key cannot come out of `to_vals`.
        (Ty::U8 | Ty::U16 | Ty::U32 | Ty::I32 | Ty::Flag | Ty::Word(_), _) => {}
    }
}

impl Step {
    /// The verb of the step.
    pub fn kind(&self) -> StepKind {
        match self {
            Step::Append { .. } => StepKind::Append,
            Step::Deliver { .. } => StepKind::Deliver,
            Step::Poll { .. } => StepKind::Poll,
            Step::Tick { .. } => StepKind::Tick,
            Step::HeadTick { .. } => StepKind::HeadTick,
            Step::Fetch { .. } => StepKind::Fetch,
            Step::Meet { .. } => StepKind::Meet,
            Step::Checkpoint { .. } => StepKind::Checkpoint,
            Step::Offsite { .. } => StepKind::Offsite,
            Step::Disaster { .. } => StepKind::Disaster,
            Step::Resend { .. } => StepKind::Resend,
            Step::Partition { .. } => StepKind::Partition,
            Step::Heal { .. } => StepKind::Heal,
            Step::Slow { .. } => StepKind::Slow,
            Step::Skew { .. } => StepKind::Skew,
            Step::Jump { .. } => StepKind::Jump,
            Step::Crash { .. } => StepKind::Crash,
            Step::Restart { .. } => StepKind::Restart,
            Step::Disk { .. } => StepKind::Disk,
            Step::Attack { .. } => StepKind::Attack,
            Step::Policy { .. } => StepKind::Policy,
            Step::Quiesce { .. } => StepKind::Quiesce,
        }
    }

    /// The salt of the step.
    pub fn salt(&self) -> u32 {
        match *self {
            Step::Append { salt, .. }
            | Step::Deliver { salt, .. }
            | Step::Poll { salt, .. }
            | Step::Tick { salt, .. }
            | Step::HeadTick { salt }
            | Step::Fetch { salt, .. }
            | Step::Meet { salt, .. }
            | Step::Checkpoint { salt, .. }
            | Step::Offsite { salt }
            | Step::Disaster { salt, .. }
            | Step::Resend { salt, .. }
            | Step::Partition { salt, .. }
            | Step::Heal { salt, .. }
            | Step::Slow { salt, .. }
            | Step::Skew { salt, .. }
            | Step::Jump { salt, .. }
            | Step::Crash { salt, .. }
            | Step::Restart { salt, .. }
            | Step::Disk { salt, .. }
            | Step::Attack { salt, .. }
            | Step::Policy { salt, .. }
            | Step::Quiesce { salt } => salt,
        }
    }

    /// The values of the keys of the verb, in the order of `StepKind::keys`.
    fn to_vals(self) -> Vec<Val> {
        match self {
            Step::Append {
                dev,
                chain,
                payload,
                ..
            } => vec![
                Val::U(u32::from(dev)),
                Val::U(u32::from(chain)),
                Val::U(u32::from(payload)),
            ],
            Step::Deliver { n, .. } => vec![Val::U(u32::from(n))],
            Step::Poll { node, n, .. } => vec![Val::U(u32::from(node)), Val::U(u32::from(n))],
            Step::Tick { secs, .. } => vec![Val::U(secs)],
            Step::HeadTick { .. } | Step::Offsite { .. } | Step::Quiesce { .. } => Vec::new(),
            Step::Fetch { dev, chain, .. } => {
                vec![Val::U(u32::from(dev)), Val::U(u32::from(chain))]
            }
            Step::Meet { a, b, .. } => vec![Val::U(u32::from(a)), Val::U(u32::from(b))],
            Step::Checkpoint { chain, .. } => vec![Val::U(u32::from(chain))],
            Step::Disaster {
                copy,
                sig,
                announce,
                ..
            } => vec![
                Val::U(u32::from(copy)),
                Val::W(sig as usize),
                Val::U(u32::from(announce)),
            ],
            Step::Resend { dev, .. } => vec![Val::U(u32::from(dev))],
            Step::Partition { mask, .. } | Step::Heal { mask, .. } => {
                vec![Val::U(u32::from(mask))]
            }
            Step::Slow { node, factor, .. } => {
                vec![Val::U(u32::from(node)), Val::U(u32::from(factor))]
            }
            Step::Skew { node, shift, .. } | Step::Jump { node, shift, .. } => {
                vec![Val::U(u32::from(node)), Val::S(shift.secs())]
            }
            Step::Crash { node, .. } => vec![Val::U(u32::from(node))],
            Step::Restart { node, wipe, .. } => {
                vec![Val::U(u32::from(node)), Val::W(usize::from(wipe))]
            }
            Step::Disk { node, fault, .. } => {
                vec![Val::U(u32::from(node)), Val::W(fault as usize)]
            }
            Step::Attack {
                kind, dev, chain, ..
            } => vec![
                Val::W(kind as usize),
                Val::U(u32::from(dev)),
                Val::U(u32::from(chain)),
            ],
            Step::Policy {
                scope,
                group,
                patch,
                ..
            } => vec![
                Val::U(u32::from(scope)),
                Val::U(u32::from(group)),
                Val::U(patch),
            ],
        }
    }

    /// The step of a verb, its parsed values and its salt.
    fn from_vals(kind: StepKind, vals: &[Val], salt: u32) -> Result<Step, StepParseError> {
        let mut v = Vals { vals, at: 0 };
        Ok(match kind {
            StepKind::Append => Step::Append {
                dev: v.u8()?,
                chain: v.u8()?,
                payload: v.u16()?,
                salt,
            },
            StepKind::Deliver => Step::Deliver { n: v.u8()?, salt },
            StepKind::Poll => Step::Poll {
                node: v.u8()?,
                n: v.u8()?,
                salt,
            },
            StepKind::Tick => Step::Tick {
                secs: v.u32()?,
                salt,
            },
            StepKind::HeadTick => Step::HeadTick { salt },
            StepKind::Fetch => Step::Fetch {
                dev: v.u8()?,
                chain: v.u8()?,
                salt,
            },
            StepKind::Meet => Step::Meet {
                a: v.u8()?,
                b: v.u8()?,
                salt,
            },
            StepKind::Checkpoint => Step::Checkpoint {
                chain: v.u8()?,
                salt,
            },
            StepKind::Offsite => Step::Offsite { salt },
            StepKind::Disaster => Step::Disaster {
                copy: v.u8()?,
                sig: *SigChoice::ALL
                    .get(v.word()?)
                    .ok_or(StepParseError::BadWord)?,
                announce: v.flag()?,
                salt,
            },
            StepKind::Resend => Step::Resend { dev: v.u8()?, salt },
            StepKind::Partition => Step::Partition {
                mask: v.u16()?,
                salt,
            },
            StepKind::Heal => Step::Heal {
                mask: v.u16()?,
                salt,
            },
            StepKind::Slow => Step::Slow {
                node: v.u8()?,
                factor: v.u8()?,
                salt,
            },
            StepKind::Skew => Step::Skew {
                node: v.u8()?,
                shift: Shift::from_secs(v.i32()?),
                salt,
            },
            StepKind::Jump => Step::Jump {
                node: v.u8()?,
                shift: Shift::from_secs(v.i32()?),
                salt,
            },
            StepKind::Crash => Step::Crash {
                node: v.u8()?,
                salt,
            },
            StepKind::Restart => Step::Restart {
                node: v.u8()?,
                wipe: v.word()? == 1,
                salt,
            },
            StepKind::Disk => Step::Disk {
                node: v.u8()?,
                fault: *DiskFault::ALL
                    .get(v.word()?)
                    .ok_or(StepParseError::BadWord)?,
                salt,
            },
            StepKind::Attack => Step::Attack {
                kind: *AttackKind::ALL
                    .get(v.word()?)
                    .ok_or(StepParseError::BadWord)?,
                dev: v.u8()?,
                chain: v.u8()?,
                salt,
            },
            StepKind::Policy => Step::Policy {
                scope: v.u8()?,
                group: v.u8()?,
                patch: v.u32()?,
                salt,
            },
            StepKind::Quiesce => Step::Quiesce { salt },
        })
    }

    /// The one-line text form of the step.
    ///
    /// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
    ///
    /// Caller: `SeedFile::to_text`, `Failure::trace_tail`, tests/x02.
    pub fn to_line(&self) -> String {
        let kind = self.kind();
        let mut out = String::from(kind.verb());
        for ((name, ty), val) in kind.keys().iter().zip(self.to_vals()) {
            out.push(' ');
            out.push_str(name);
            out.push('=');
            write_value(*ty, val, &mut out);
        }
        out.push_str(" salt=");
        write_value(Ty::U32, Val::U(self.salt()), &mut out);
        out
    }

    /// The step of a line written by [`Step::to_line`]; anything else is a typed error.
    ///
    /// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
    ///
    /// Caller: `SeedFile::parse`, tests/x02.
    pub fn parse(line: &str) -> Result<Step, StepParseError> {
        if line.is_empty() {
            return Err(StepParseError::Empty);
        }
        if line.starts_with(' ') || line.ends_with(' ') {
            return Err(StepParseError::EdgeSpace);
        }
        if line.contains("  ") {
            return Err(StepParseError::DoubleSpace);
        }
        let mut tokens = line.split(' ');
        let verb = tokens.next().ok_or(StepParseError::Empty)?;
        let kind = StepKind::from_verb(verb).ok_or(StepParseError::UnknownVerb)?;
        let keys = kind.keys();
        let mut vals = Vec::with_capacity(keys.len());
        let mut seen: Vec<&str> = Vec::with_capacity(keys.len() + 1);
        for (expected, ty) in keys {
            let token = tokens.next().ok_or(StepParseError::MissingKey)?;
            let value = take_key(token, expected, &seen, keys)?;
            vals.push(parse_value(*ty, value)?);
            seen.push(expected);
        }
        let token = tokens.next().ok_or(StepParseError::MissingSalt)?;
        let value = take_key(token, "salt", &seen, keys)?;
        let salt = match parse_value(Ty::U32, value)? {
            Val::U(v) => v,
            Val::S(_) | Val::W(_) => return Err(StepParseError::BadNumber),
        };
        if tokens.next().is_some() {
            return Err(StepParseError::TrailingText);
        }
        Step::from_vals(kind, &vals, salt)
    }
}

/// The value of the token `key=value` when its key is the `expected` one; the typed error of
/// the way it is wrong otherwise.
fn take_key<'a>(
    token: &'a str,
    expected: &str,
    seen: &[&str],
    keys: &[(&str, Ty)],
) -> Result<&'a str, StepParseError> {
    let (key, value) = token.split_once('=').ok_or(StepParseError::NotKeyValue)?;
    if key == expected {
        return Ok(value);
    }
    if seen.contains(&key) {
        return Err(StepParseError::RepeatedKey);
    }
    if key == "salt" || keys.iter().any(|(name, _)| *name == key) {
        return Err(StepParseError::KeyOutOfOrder);
    }
    Err(StepParseError::UnknownKey)
}

/// A step drawn uniformly over the verbs with random fields: every verb, any value of its keys.
/// The generator of the scenarios reads the world; this one does not, it exists for the codec's
/// round-trip test.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
///
/// Caller: tests/x02_step_and_seed_text_is_strict_and_round_trips.rs.
pub fn arbitrary(rng: &mut Rng) -> Step {
    let kind = StepKind::ALL
        .get(below(rng, StepKind::ALL.len() as u64) as usize)
        .copied()
        .unwrap_or(StepKind::Quiesce);
    let salt = rng.next_u64() as u32;
    let a = rng.next_u64();
    let b = rng.next_u64();
    let byte = |x: u64| (x & 0xff) as u8;
    match kind {
        StepKind::Append => Step::Append {
            dev: byte(a),
            chain: byte(a >> 8),
            payload: (b & 0xffff) as u16,
            salt,
        },
        StepKind::Deliver => Step::Deliver { n: byte(a), salt },
        StepKind::Poll => Step::Poll {
            node: byte(a),
            n: byte(a >> 8),
            salt,
        },
        StepKind::Tick => Step::Tick {
            secs: (a & 0xffff_ffff) as u32,
            salt,
        },
        StepKind::HeadTick => Step::HeadTick { salt },
        StepKind::Fetch => Step::Fetch {
            dev: byte(a),
            chain: byte(a >> 8),
            salt,
        },
        StepKind::Meet => Step::Meet {
            a: byte(a),
            b: byte(a >> 8),
            salt,
        },
        StepKind::Checkpoint => Step::Checkpoint {
            chain: byte(a),
            salt,
        },
        StepKind::Offsite => Step::Offsite { salt },
        StepKind::Disaster => Step::Disaster {
            copy: byte(a),
            sig: SigChoice::ALL
                .get((b % 4) as usize)
                .copied()
                .unwrap_or(SigChoice::Map),
            announce: a & 0x100 != 0,
            salt,
        },
        StepKind::Resend => Step::Resend { dev: byte(a), salt },
        StepKind::Partition => Step::Partition {
            mask: (a & 0xffff) as u16,
            salt,
        },
        StepKind::Heal => Step::Heal {
            mask: (a & 0xffff) as u16,
            salt,
        },
        StepKind::Slow => Step::Slow {
            node: byte(a),
            factor: byte(a >> 8),
            salt,
        },
        StepKind::Skew => Step::Skew {
            node: byte(a),
            shift: Shift::from_secs((b & 0xffff_ffff) as u32 as i32),
            salt,
        },
        StepKind::Jump => Step::Jump {
            node: byte(a),
            shift: Shift::from_secs((b & 0xffff_ffff) as u32 as i32),
            salt,
        },
        StepKind::Crash => Step::Crash {
            node: byte(a),
            salt,
        },
        StepKind::Restart => Step::Restart {
            node: byte(a),
            wipe: b & 1 == 1,
            salt,
        },
        StepKind::Disk => Step::Disk {
            node: byte(a),
            fault: DiskFault::ALL
                .get((b % 5) as usize)
                .copied()
                .unwrap_or(DiskFault::Lie),
            salt,
        },
        StepKind::Attack => Step::Attack {
            kind: AttackKind::ALL
                .get((b % 20) as usize)
                .copied()
                .unwrap_or(AttackKind::PeerJunk),
            dev: byte(a),
            chain: byte(a >> 8),
            salt,
        },
        StepKind::Policy => Step::Policy {
            scope: byte(a),
            group: byte(a >> 8),
            patch: (b & 0xffff_ffff) as u32,
            salt,
        },
        StepKind::Quiesce => Step::Quiesce { salt },
    }
}
