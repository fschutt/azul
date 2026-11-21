//! The notices of a decision: the statement of reasons to the affected account and the notifier's receipt and
//! decision. Both are plain bodies (deterministic CBOR) that the caller seals and drops into the inbox space of
//! their reader through its request (planning/architecture.md section 7.10: "a sealed notice dropped into their
//! inbox space through its request"; section 5.12 requests).
//!
//! Implements planning/architecture.md section 3.9 ("Telling people: the notifier gets a receipt number at once
//! and the decision in the app. The affected account gets a statement of reasons in its inbox space as a notice
//! (7.10): what was restricted, why, on what basis, and how to object (a reply through the app, decided by the
//! operator). Accounts have no email (D37), so the inbox is the channel") and the two articles it serves: DSA
//! Art. 17 (the statement of reasons: the restriction, the facts and circumstances, the use of automated means,
//! the legal or contractual ground, the possibilities of redress) and Art. 16(5) (the notifier is told the
//! decision). The statement is STRUCTURED, keys and codes and no prose: the app localises it.
//!
//! The wire form of both is a CBOR map with unsigned keys: 1 version, 2 notice kind (1 statement, 2 notifier
//! notice), then the fields. A reader ignores unknown keys that are not in `crit` and refuses an unknown `crit`
//! key (section 5.1 principle 6).
//!
//! Caller: S46-INT-3 (AzCtl builds and seals the notices when the operator decides), S46-INT-2 (the leader task
//! builds the statement of an automatic step), the apps (read them from the inbox).

use azlin_cbor::{decode, encode, IntMap, IntMapBuilder, Limits, ShapeError, Value};

use crate::body::{BodyError, Field, Reason, Text};
use crate::decision::{Action, Decision};
use crate::ids::{Day, ReportId};
use crate::ledger::AutoStep;

const NOTICE_VERSION: u64 = 1;
const KIND_STATEMENT: u64 = 1;
const KIND_NOTIFIER: u64 = 2;

const K_VERSION: u64 = 1;
const K_KIND: u64 = 2;

const K_RESTRICTION: u64 = 3;
const K_FACTS: u64 = 4;
const K_AUTOMATED: u64 = 5;
const K_GROUND: u64 = 6;
const K_REDRESS: u64 = 7;
const K_DAY: u64 = 8;

const K_RECEIPT: u64 = 3;
const K_OUTCOME: u64 = 4;
const K_NOTIFIER_DAY: u64 = 5;

const F_KIND: u64 = 1;
const F_REASON: u64 = 2;
const F_TEXT: u64 = 3;
const F_DEVICES: u64 = 2;
const FACTS_NOTICE: u64 = 1;
const FACTS_COUNTED: u64 = 2;

/// A notice is a few hundred bytes: the operator's text (2,000 bytes at most) and a dozen small fields.
const NOTICE_LIMITS: Limits = Limits {
    max_input: 4 * 1024,
    max_depth: 4,
    max_items: 32,
    max_total_items: 64,
    max_text: 4 * 1024,
};

/// What was restricted (DSA Art. 17(3)(a)): the measure the account or its link suffers.
///
/// Caller: S46-INT-3, S46-INT-2 (through `statement_of`, `statement_of_step`), the apps (they pick the sentence).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Restriction {
    /// Nothing was restricted: the operator looked and took no action.
    NoRestriction = 1,
    /// A link is paused until the operator decides (the automatic step).
    LinkPaused = 2,
    /// The account's public (`anyone`) links are paused until the operator switches them back on
    /// (`links_disabled`, the automatic step).
    PublicLinksDisabled = 3,
    /// A link is blocked (`link_blocks`, every node at once).
    LinkBlocked = 4,
    /// The account is read only.
    AccountReadOnly = 5,
    /// The account is banned (`ban_until`).
    AccountBanned = 6,
    /// A legal hold: the data is kept (no purge) until the hold is released.
    DataKept = 7,
    /// The case was referred to the authorities (Art. 18).
    Referred = 8,
}

impl Restriction {
    /// Every restriction, in wire order.
    pub const ALL: [Restriction; 8] = [
        Restriction::NoRestriction,
        Restriction::LinkPaused,
        Restriction::PublicLinksDisabled,
        Restriction::LinkBlocked,
        Restriction::AccountReadOnly,
        Restriction::AccountBanned,
        Restriction::DataKept,
        Restriction::Referred,
    ];

    /// The wire value.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The restriction of a wire value: `UnknownRestriction` for any other number.
    pub fn from_code(code: u64) -> Result<Restriction, BodyError> {
        Restriction::ALL
            .into_iter()
            .find(|r| u64::from(r.code()) == code)
            .ok_or(BodyError::UnknownRestriction(code))
    }

    /// The restriction a decision's action has.
    pub const fn of(action: Action) -> Restriction {
        match action {
            Action::NoAction => Restriction::NoRestriction,
            Action::BlockLink => Restriction::LinkBlocked,
            Action::LockAccount => Restriction::AccountReadOnly,
            Action::Ban => Restriction::AccountBanned,
            Action::LegalHold => Restriction::DataKept,
            Action::ReferToAuthorities => Restriction::Referred,
        }
    }

    /// Whether the restriction is one the affected account SEES: a paused, blocked, locked or banned link or
    /// account. The statement of reasons goes into the account's inbox only for those: a legal hold and a
    /// referral restrict nothing the account notices, and telling it could warn the person an authority looks at
    /// (provisional answer to drift X-DELTA-W1S46A-12: the plan's "what was restricted" is read as a restriction).
    pub const fn is_visible_to_account(self) -> bool {
        matches!(
            self,
            Restriction::LinkPaused
                | Restriction::PublicLinksDisabled
                | Restriction::LinkBlocked
                | Restriction::AccountReadOnly
                | Restriction::AccountBanned
        )
    }
}

/// On what basis (DSA Art. 17(3)(d)): the legal or the contractual ground, as a key.
///
/// Caller: S46-INT-3, the apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Ground {
    /// The content is illegal under Union or Member State law (illegal content, CSAM, terrorism, copyright).
    IllegalContent = 1,
    /// The terms of use forbid it (malware, phishing, anything else).
    Terms = 2,
    /// The automatic safeguard of the counting engine: a temporary measure until a human decides (Art. 16).
    AutomaticSafeguard = 3,
}

impl Ground {
    /// Every ground, in wire order.
    pub const ALL: [Ground; 3] = [
        Ground::IllegalContent,
        Ground::Terms,
        Ground::AutomaticSafeguard,
    ];

    /// The wire value.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The ground of a wire value: `UnknownGround` for any other number.
    pub fn from_code(code: u64) -> Result<Ground, BodyError> {
        Ground::ALL
            .into_iter()
            .find(|g| u64::from(g.code()) == code)
            .ok_or(BodyError::UnknownGround(code))
    }

    /// The ground a reason rests on: the four reasons that name content the law forbids rest on the law, the
    /// others on the terms.
    pub const fn of(reason: Reason) -> Ground {
        match reason {
            Reason::IllegalContent | Reason::Csam | Reason::Terrorism | Reason::Copyright => {
                Ground::IllegalContent
            }
            Reason::Malware | Reason::Phishing | Reason::Other => Ground::Terms,
        }
    }
}

/// How to object (DSA Art. 17(3)(f)): the paths of redress the account is told about.
///
/// Caller: S46-INT-3, the apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Redress {
    /// A reply through the app, decided by the operator (the plan's way: planning/architecture.md section 3.9).
    pub reply_in_app: bool,
    /// Out-of-court dispute settlement (not offered at launch).
    pub out_of_court: bool,
    /// Judicial redress.
    pub courts: bool,
}

impl Redress {
    /// What every statement names: a reply through the app, and the courts.
    pub const STANDARD: Redress = Redress {
        reply_in_app: true,
        out_of_court: false,
        courts: true,
    };

    /// The wire word: bit 0 the reply, bit 1 out-of-court, bit 2 the courts.
    pub const fn bits(self) -> u64 {
        (self.reply_in_app as u64) | ((self.out_of_court as u64) << 1) | ((self.courts as u64) << 2)
    }

    /// The redress of a wire word: `BadRedress` for 0 (no path at all) and for a bit that is not defined.
    pub const fn from_bits(bits: u64) -> Result<Redress, BodyError> {
        if bits == 0 || bits > 7 {
            return Err(BodyError::BadRedress(bits));
        }
        Ok(Redress {
            reply_in_app: bits & 1 != 0,
            out_of_court: bits & 2 != 0,
            courts: bits & 4 != 0,
        })
    }
}

/// The facts and circumstances (DSA Art. 17(3)(b)): what the decision rests on, without naming the notifier
/// (reports are anonymous: Art. 17(3)(b) asks for the identity only where strictly necessary).
///
/// Caller: S46-INT-3, S46-INT-2, the apps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Facts {
    /// The operator decided after a report (a notice under Art. 16): the reason of the decision, and the
    /// operator's text.
    Notice {
        /// The reason of the decision.
        reason: Reason,
        /// The operator's text, if there is one.
        text: Option<Text>,
    },
    /// The counting engine acted by itself: this many different devices reported; no content was looked at
    /// (planning/architecture.md section 3.9: "counting is per link and per account, from reports only, and no
    /// content is looked at").
    Counted {
        /// The different devices that reported.
        devices: u32,
    },
}

impl Facts {
    fn read(value: &Value) -> Result<Facts, BodyError> {
        let mut map = IntMap::new(value)?;
        match map.req_u64(F_KIND)? {
            FACTS_NOTICE => {
                let reason = Reason::from_code(map.req_u64(F_REASON)?)?;
                let text = map
                    .opt_text(F_TEXT)?
                    .map(|s| Text::for_field(s, Field::Text))
                    .transpose()?;
                map.finish()?;
                Ok(Facts::Notice { reason, text })
            }
            FACTS_COUNTED => {
                let devices = u32::try_from(map.req_u64(F_DEVICES)?)
                    .map_err(|_| BodyError::Shape(ShapeError::WrongType(F_DEVICES)))?;
                map.finish()?;
                Ok(Facts::Counted { devices })
            }
            other => Err(BodyError::UnknownFacts(other)),
        }
    }

    fn to_value(&self) -> Result<Value, BodyError> {
        let builder = IntMapBuilder::new();
        let builder = match self {
            Facts::Notice { reason, text } => {
                let builder = builder
                    .put(F_KIND, Value::Uint(FACTS_NOTICE))
                    .put(F_REASON, Value::Uint(u64::from(reason.code())));
                match text {
                    Some(text) => {
                        if text.len() > Field::Text.max() {
                            return Err(BodyError::TooLong(Field::Text));
                        }
                        builder.put(F_TEXT, Value::Text(text.as_str().to_owned()))
                    }
                    None => builder,
                }
            }
            Facts::Counted { devices } => builder
                .put(F_KIND, Value::Uint(FACTS_COUNTED))
                .put(F_DEVICES, Value::Uint(u64::from(*devices))),
        };
        Ok(builder.build()?)
    }
}

/// A day read from a wire number: a number beyond `u32` is no day.
fn read_day(map: &mut IntMap<'_>, key: u64) -> Result<Day, BodyError> {
    let n = map.req_u64(key)?;
    let days = u32::try_from(n).map_err(|_| BodyError::Shape(ShapeError::WrongType(key)))?;
    Ok(Day::new(days))
}

/// The head of a notice: the version and the kind, checked.
fn read_head(map: &mut IntMap<'_>, kind: u64) -> Result<(), BodyError> {
    let version = map.req_u64(K_VERSION)?;
    if version != NOTICE_VERSION {
        return Err(BodyError::Version(version));
    }
    let found = map.req_u64(K_KIND)?;
    if found != kind {
        return Err(BodyError::WrongNotice(found));
    }
    Ok(())
}

/// The statement of reasons of DSA Art. 17, structured: what was restricted, the facts, whether automated means
/// decided, the ground, how to object. Encoded as the inbox notice of section 7.10.
///
/// Caller: S46-INT-3 and S46-INT-2 (they build, encode and seal it), the apps (they decode it from the inbox).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementOfReasons {
    /// What was restricted.
    pub restriction: Restriction,
    /// The facts and circumstances.
    pub facts: Facts,
    /// Whether automated means decided: true exactly for the counting engine's step (Art. 17(3)(c)).
    pub automated: bool,
    /// On what basis.
    pub ground: Ground,
    /// How to object.
    pub redress: Redress,
    /// The day of the decision.
    pub day: Day,
}

/// The statement of reasons of a decision of the operator. `automated` is what the CALLER says: false for the
/// operator's own decision, true only for the counting engine's step (see [`statement_of_step`], which builds
/// that statement and says true itself).
///
/// Caller: S46-INT-3 (AzCtl builds it when the operator records a decision that restricts the account).
pub fn statement_of(decision: &Decision, automated: bool) -> StatementOfReasons {
    let ground = if decision.action == Action::ReferToAuthorities {
        Ground::IllegalContent
    } else {
        Ground::of(decision.reason)
    };
    StatementOfReasons {
        restriction: Restriction::of(decision.action),
        facts: Facts::Notice {
            reason: decision.reason,
            text: decision.text.clone(),
        },
        automated,
        ground,
        redress: Redress::STANDARD,
        day: decision.day,
    }
}

/// The statement of reasons of an automatic step of the counting engine (drift X-DELTA-W1S46A-14: the brief's
/// `statement_of` cannot say it): the pause of a link or the loss of
/// the account's public links, decided by automated means (Art. 17(3)(c)) from the number of devices alone.
///
/// Caller: S46-INT-2 (the leader task tells the owner why in the inbox: planning/architecture.md section 3.9:
/// "its owner is told why in the inbox").
pub fn statement_of_step(step: &AutoStep, day: Day) -> StatementOfReasons {
    let (restriction, devices) = match step {
        AutoStep::PauseLink { devices, .. } => (Restriction::LinkPaused, *devices),
        AutoStep::DisableLinks { devices, .. } => (Restriction::PublicLinksDisabled, *devices),
    };
    StatementOfReasons {
        restriction,
        facts: Facts::Counted { devices },
        automated: true,
        ground: Ground::AutomaticSafeguard,
        redress: Redress::STANDARD,
        day,
    }
}

impl StatementOfReasons {
    /// The deterministic encoding of the statement.
    ///
    /// Caller: S46-INT-3, S46-INT-2 (before sealing the notice).
    pub fn encode(&self) -> Result<Vec<u8>, BodyError> {
        let value = IntMapBuilder::new()
            .put(K_VERSION, Value::Uint(NOTICE_VERSION))
            .put(K_KIND, Value::Uint(KIND_STATEMENT))
            .put(
                K_RESTRICTION,
                Value::Uint(u64::from(self.restriction.code())),
            )
            .put(K_FACTS, self.facts.to_value()?)
            .put(K_AUTOMATED, Value::Bool(self.automated))
            .put(K_GROUND, Value::Uint(u64::from(self.ground.code())))
            .put(K_REDRESS, Value::Uint(self.redress.bits()))
            .put(K_DAY, Value::Uint(u64::from(self.day.get())))
            .build()?;
        Ok(encode(&value, &NOTICE_LIMITS)?)
    }

    /// The statement of `bytes`, which must be the strict encoding of one: version 1, kind 1, every field, a
    /// known code in every place that has codes. Unknown keys that are not in `crit` are ignored.
    ///
    /// Caller: the apps (the inbox's notice reader), S46-INT-3 (tests of the round trip).
    pub fn decode(bytes: &[u8]) -> Result<StatementOfReasons, BodyError> {
        let value = decode(bytes, &NOTICE_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        read_head(&mut map, KIND_STATEMENT)?;
        let restriction = Restriction::from_code(map.req_u64(K_RESTRICTION)?)?;
        let facts = Facts::read(map.req_value(K_FACTS)?)?;
        let automated = map.req_bool(K_AUTOMATED)?;
        let ground = Ground::from_code(map.req_u64(K_GROUND)?)?;
        let redress = Redress::from_bits(map.req_u64(K_REDRESS)?)?;
        let day = read_day(&mut map, K_DAY)?;
        map.finish()?;
        Ok(StatementOfReasons {
            restriction,
            facts,
            automated,
            ground,
            redress,
            day,
        })
    }
}

/// What the notifier is told (DSA Art. 16(5); drift X-DELTA-W1S46A-19): one word. Nothing in it names the reported account, the measure
/// or who was affected: the notifier learns only whether the operator acted.
///
/// Caller: S46-INT-3, the apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Outcome {
    /// The operator looked and took no action.
    NoAction = 1,
    /// The operator acted (whatever the measure: the notifier is not told which).
    ActionTaken = 2,
}

impl Outcome {
    /// Every outcome, in wire order.
    pub const ALL: [Outcome; 2] = [Outcome::NoAction, Outcome::ActionTaken];

    /// The wire value.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The outcome of a wire value: `UnknownOutcome` for any other number.
    pub fn from_code(code: u64) -> Result<Outcome, BodyError> {
        Outcome::ALL
            .into_iter()
            .find(|o| u64::from(o.code()) == code)
            .ok_or(BodyError::UnknownOutcome(code))
    }

    /// The outcome a decision's action has for the notifier.
    pub const fn of(action: Action) -> Outcome {
        match action {
            Action::NoAction => Outcome::NoAction,
            Action::BlockLink
            | Action::LockAccount
            | Action::Ban
            | Action::LegalHold
            | Action::ReferToAuthorities => Outcome::ActionTaken,
        }
    }
}

/// The notifier's notice (DSA Art. 16(5)): the receipt number the reporter's app made, and the outcome.
///
/// Caller: S46-INT-3 (AzCtl drops it into the reporter's inbox, through the reply handle of the report).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotifierNotice {
    /// The receipt number of the report.
    pub receipt: ReportId,
    /// What was decided, as one word.
    pub outcome: Outcome,
    /// The day of the decision.
    pub day: Day,
}

impl NotifierNotice {
    /// The notice for a decision: its report's receipt, the outcome word of its action, its day.
    ///
    /// Caller: S46-INT-3.
    pub fn for_decision(decision: &Decision) -> NotifierNotice {
        NotifierNotice {
            receipt: decision.report,
            outcome: Outcome::of(decision.action),
            day: decision.day,
        }
    }

    /// The deterministic encoding of the notice.
    ///
    /// Caller: S46-INT-3 (before sealing the notice).
    pub fn encode(&self) -> Result<Vec<u8>, BodyError> {
        let value = IntMapBuilder::new()
            .put(K_VERSION, Value::Uint(NOTICE_VERSION))
            .put(K_KIND, Value::Uint(KIND_NOTIFIER))
            .put(K_RECEIPT, Value::Bytes(self.receipt.as_bytes().to_vec()))
            .put(K_OUTCOME, Value::Uint(u64::from(self.outcome.code())))
            .put(K_NOTIFIER_DAY, Value::Uint(u64::from(self.day.get())))
            .build()?;
        Ok(encode(&value, &NOTICE_LIMITS)?)
    }

    /// The notice of `bytes`, which must be the strict encoding of one (version 1, kind 2). Unknown keys that
    /// are not in `crit` are ignored.
    ///
    /// Caller: the apps (the reporter's inbox reader).
    pub fn decode(bytes: &[u8]) -> Result<NotifierNotice, BodyError> {
        let value = decode(bytes, &NOTICE_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        read_head(&mut map, KIND_NOTIFIER)?;
        let receipt = ReportId::from_bytes(map.req_bytes_n::<16>(K_RECEIPT)?);
        let outcome = Outcome::from_code(map.req_u64(K_OUTCOME)?)?;
        let day = read_day(&mut map, K_NOTIFIER_DAY)?;
        map.finish()?;
        Ok(NotifierNotice {
            receipt,
            outcome,
            day,
        })
    }
}
