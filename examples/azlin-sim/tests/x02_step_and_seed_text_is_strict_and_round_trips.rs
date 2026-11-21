//! X02: the text of a step is strict and round-trips (planning/architecture.md section 1.4 rule 2:
//! a failing seed replays exactly). The seed-file half of the check is in `seeds.rs` of the same
//! check number (unit U5).

use azlin_seams::sim::Rng;
use azlin_sim::step::{
    arbitrary, AttackKind, DiskFault, SigChoice, Step, StepKind, StepParseError,
};
use azlin_sim::time::Shift;

#[test]
fn x02_ten_thousand_generated_steps_round_trip() {
    let mut rng = Rng::new(0x5EED_0002);
    for case in 0..10_000 {
        let step = arbitrary(&mut rng);
        let line = step.to_line();
        assert_eq!(
            Step::parse(&line),
            Ok(step),
            "case {case} did not round-trip: {line}"
        );
        // The form is canonical: writing what was parsed gives the same line.
        if let Ok(again) = Step::parse(&line) {
            assert_eq!(again.to_line(), line, "case {case}");
        }
    }
}

#[test]
fn x02_every_verb_and_every_word_is_reachable() {
    let mut seen = Vec::new();
    for kind in StepKind::ALL {
        let step = match kind {
            StepKind::Append => Step::Append {
                dev: 1,
                chain: 2,
                payload: 3,
                salt: 4,
            },
            StepKind::Deliver => Step::Deliver { n: 1, salt: 4 },
            StepKind::Poll => Step::Poll {
                node: 1,
                n: 2,
                salt: 4,
            },
            StepKind::Tick => Step::Tick { secs: 90, salt: 4 },
            StepKind::HeadTick => Step::HeadTick { salt: 4 },
            StepKind::Fetch => Step::Fetch {
                dev: 1,
                chain: 2,
                salt: 4,
            },
            StepKind::Meet => Step::Meet {
                a: 1,
                b: 2,
                salt: 4,
            },
            StepKind::Checkpoint => Step::Checkpoint { chain: 1, salt: 4 },
            StepKind::Offsite => Step::Offsite { salt: 4 },
            StepKind::Disaster => Step::Disaster {
                copy: 1,
                sig: SigChoice::Both,
                announce: true,
                salt: 4,
            },
            StepKind::Resend => Step::Resend { dev: 1, salt: 4 },
            StepKind::Partition => Step::Partition {
                mask: 0x0102,
                salt: 4,
            },
            StepKind::Heal => Step::Heal {
                mask: 0x0102,
                salt: 4,
            },
            StepKind::Slow => Step::Slow {
                node: 1,
                factor: 3,
                salt: 4,
            },
            StepKind::Skew => Step::Skew {
                node: 1,
                shift: Shift::from_secs(-30),
                salt: 4,
            },
            StepKind::Jump => Step::Jump {
                node: 1,
                shift: Shift::from_secs(i32::MIN),
                salt: 4,
            },
            StepKind::Crash => Step::Crash { node: 1, salt: 4 },
            StepKind::Restart => Step::Restart {
                node: 1,
                wipe: true,
                salt: 4,
            },
            StepKind::Disk => Step::Disk {
                node: 1,
                fault: DiskFault::Vanish,
                salt: 4,
            },
            StepKind::Attack => Step::Attack {
                kind: AttackKind::PeerFork,
                dev: 1,
                chain: 2,
                salt: 4,
            },
            StepKind::Policy => Step::Policy {
                scope: 1,
                group: 2,
                patch: 0xdead_beef,
                salt: 4,
            },
            StepKind::Quiesce => Step::Quiesce { salt: 4 },
        };
        assert_eq!(step.kind(), kind);
        assert_eq!(step.salt(), 4);
        assert!(
            step.to_line().starts_with(kind.verb()),
            "{}",
            step.to_line()
        );
        assert_eq!(Step::parse(&step.to_line()), Ok(step));
        assert!(!seen.contains(&kind.verb()), "verb {} twice", kind.verb());
        seen.push(kind.verb());
    }
    assert_eq!(seen.len(), 22);
    for (index, kind) in AttackKind::ALL.iter().enumerate() {
        assert_eq!(
            *kind as usize, index,
            "the index of a kind is its position in ALL"
        );
        assert_eq!(AttackKind::from_name(kind.name()), Some(*kind));
        let step = Step::Attack {
            kind: *kind,
            dev: 0,
            chain: 0,
            salt: 0,
        };
        assert_eq!(Step::parse(&step.to_line()), Ok(step), "{}", kind.name());
    }
    for (index, fault) in DiskFault::ALL.iter().enumerate() {
        assert_eq!(*fault as usize, index);
        let step = Step::Disk {
            node: 0,
            fault: *fault,
            salt: 0,
        };
        assert_eq!(Step::parse(&step.to_line()), Ok(step), "{}", fault.name());
    }
    for (index, sig) in SigChoice::ALL.iter().enumerate() {
        assert_eq!(*sig as usize, index);
        let step = Step::Disaster {
            copy: 0,
            sig: *sig,
            announce: false,
            salt: 0,
        };
        assert_eq!(Step::parse(&step.to_line()), Ok(step), "{}", sig.name());
    }
}

#[test]
fn x02_the_example_of_the_brief_is_the_text_form() {
    let step = Step::Append {
        dev: 1,
        chain: 0,
        payload: 3,
        salt: 0xa1b2,
    };
    assert_eq!(
        step.to_line(),
        "append dev=0x01 chain=0x00 payload=0x0003 salt=0x0000a1b2"
    );
    let step = Step::Disaster {
        copy: 0,
        sig: SigChoice::Map,
        announce: false,
        salt: 7,
    };
    assert_eq!(
        step.to_line(),
        "disaster copy=0x00 sig=map announce=0 salt=0x00000007"
    );
    let step = Step::Jump {
        node: 2,
        shift: Shift::from_secs(-3600),
        salt: 9,
    };
    assert_eq!(step.to_line(), "jump node=0x02 secs=-3600 salt=0x00000009");
}

#[test]
fn x02_thirty_malformed_lines_each_fail_with_their_own_error() {
    use StepParseError::*;
    let cases: &[(&str, StepParseError)] = &[
        ("", Empty),
        (" deliver n=0x01 salt=0x00000001", EdgeSpace),
        ("deliver n=0x01 salt=0x00000001 ", EdgeSpace),
        ("deliver  n=0x01 salt=0x00000001", DoubleSpace),
        ("flush n=0x01 salt=0x00000001", UnknownVerb),
        ("Deliver n=0x01 salt=0x00000001", UnknownVerb),
        ("deliver", MissingKey),
        ("append dev=0x01", MissingKey),
        ("append dev=0x01 chain=0x00 salt=0x00000001", KeyOutOfOrder),
        ("deliver n=0x01", MissingSalt),
        ("head_tick", MissingSalt),
        ("deliver n=0x01 salt", NotKeyValue),
        ("deliver m=0x01 salt=0x00000001", UnknownKey),
        ("poll n=0x01 node=0x00 salt=0x00000001", KeyOutOfOrder),
        ("deliver n=0x01 n=0x02 salt=0x00000001", RepeatedKey),
        ("deliver n=0x01 salt=0x00000001 extra", TrailingText),
        (
            "deliver n=0x01 salt=0x00000001 salt=0x00000002",
            TrailingText,
        ),
        ("deliver n=1 salt=0x00000001", BadNumber),
        ("deliver n=0xzz salt=0x00000001", BadNumber),
        ("deliver n=0x1ff salt=0x00000001", BadWidth),
        ("deliver n=0x01 salt=0x0001", BadWidth),
        ("deliver n=0xAB salt=0x00000001", UpperCaseHex),
        ("deliver n=-0x01 salt=0x00000001", NegativeUnsigned),
        ("tick secs=-5 salt=0x00000001", NegativeUnsigned),
        (
            "skew node=0x00 secs=99999999999 salt=0x00000001",
            OutOfRange,
        ),
        ("skew node=0x00 secs=007 salt=0x00000001", BadNumber),
        ("skew node=0x00 secs=-0 salt=0x00000001", BadNumber),
        ("skew node=0x00 secs=+5 salt=0x00000001", BadNumber),
        (
            "disaster copy=0x00 sig=nobody announce=1 salt=0x00000001",
            BadWord,
        ),
        (
            "disaster copy=0x00 sig=map announce=2 salt=0x00000001",
            OutOfRange,
        ),
        (
            "disaster copy=0x00 sig=map announce=-1 salt=0x00000001",
            NegativeUnsigned,
        ),
        (
            "attack kind=flood dev=0x00 chain=0x00 salt=0x00000001",
            BadWord,
        ),
        ("restart node=0x00 disk=maybe salt=0x00000001", BadWord),
    ];
    assert!(cases.len() >= 30, "{} cases", cases.len());
    for (line, expected) in cases {
        assert_eq!(Step::parse(line), Err(*expected), "line: {line:?}");
    }
    // The errors of the table are not one error with many names: at least 15 different ones.
    let mut distinct: Vec<StepParseError> = Vec::new();
    for (_, expected) in cases {
        if !distinct.contains(expected) {
            distinct.push(*expected);
        }
    }
    assert!(distinct.len() >= 15, "{} distinct errors", distinct.len());
}

#[test]
fn x02_the_text_of_an_error_is_a_sentence() {
    for error in [
        StepParseError::Empty,
        StepParseError::BadWidth,
        StepParseError::TrailingText,
    ] {
        let text = error.to_string();
        assert!(text.len() > 5, "{text}");
    }
}

// ---------------------------------------------------------------------------------------------
// The seed-file half of X02
// ---------------------------------------------------------------------------------------------

use azlin_sim::{
    directed_named, small_world, Expect, InvariantId, SeedFile, SeedParseError, Stage,
};

fn sample() -> SeedFile {
    let steps = directed_named("restore_story").map_or_else(Vec::new, |d| (d.steps)());
    SeedFile {
        name: String::from("sample-seed"),
        stage: Stage::S1,
        seed: 0x1a4,
        config: small_world(),
        invariant: InvariantId::RestoreTheLostTimelineStaysLost,
        expect: Expect::Pass,
        note: String::from("a note: with a colon"),
        steps,
    }
}

#[test]
fn x02_a_seed_file_round_trips() {
    let file = sample();
    assert!(file.steps.len() > 5);
    let text = file.to_text();
    assert!(
        text.starts_with(
            "azlin-sim-seed v1\nname: sample-seed\nstage: 1\nseed: 0x00000000000001a4\n"
        ),
        "{text}"
    );
    assert_eq!(SeedFile::parse(&text), Ok(file.clone()));
    // A configuration with a non-honest leader and a start epoch keeps its optional keys.
    let mut other = sample();
    other.config = other
        .config
        .with_leader(azlin_sim::LeaderVariant::DoubleSign)
        .with_start_epoch(7);
    other.expect = Expect::Fails;
    assert_eq!(SeedFile::parse(&other.to_text()), Ok(other));
}

#[test]
fn x02_malformed_seed_files_are_refused_with_a_typed_error() {
    let good = sample().to_text();
    let replace = |from: &str, to: &str| good.replacen(from, to, 1);
    let cases: Vec<(String, SeedParseError)> = vec![
        (String::new(), SeedParseError::BadHeader),
        (
            replace("azlin-sim-seed v1", "azlin-sim-seed v2"),
            SeedParseError::BadHeader,
        ),
        (
            replace("name: sample-seed", "name: Sample Seed"),
            SeedParseError::BadName,
        ),
        (replace("stage: 1", "stage: 9"), SeedParseError::BadStage),
        (
            replace("seed: 0x00000000000001a4", "seed: 0x00000000000001A4"),
            SeedParseError::BadSeed,
        ),
        (
            replace("seed: 0x00000000000001a4", "seed: 0x1a4"),
            SeedParseError::BadSeed,
        ),
        (
            replace(
                "invariant: inv_restore_the_lost_timeline_stays_lost",
                "invariant: inv_nothing",
            ),
            SeedParseError::UnknownInvariant,
        ),
        (
            replace("expect: pass", "expect: maybe"),
            SeedParseError::BadExpect,
        ),
        (replace("steps: ", "steps: x"), SeedParseError::BadCount),
    ];
    for (text, expected) in &cases {
        assert_eq!(SeedFile::parse(text), Err(expected.clone()), "{text}");
    }
    // An unknown key, a missing `steps:` count, a count that does not match, a malformed step, trailing text.
    assert!(matches!(
        SeedFile::parse(&replace("note: ", "notes: ")),
        Err(SeedParseError::UnknownKey(_))
    ));
    let without_count: String = good
        .lines()
        .filter(|l| !l.starts_with("steps:"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(
        SeedFile::parse(&without_count),
        Err(SeedParseError::UnknownKey(_) | SeedParseError::MissingKey(_))
    ));
    let count = sample().steps.len();
    let wrong_count = replace(&format!("steps: {count}"), &format!("steps: {}", count + 1));
    assert!(matches!(
        SeedFile::parse(&wrong_count),
        Err(SeedParseError::StepCountMismatch { .. })
    ));
    let bad_step = good.replacen("quiesce salt=", "quiesce  salt=", 1);
    assert!(matches!(
        SeedFile::parse(&bad_step),
        Err(SeedParseError::BadStep(_, _))
    ));
    let trailing = format!("{good}append dev=0x00 chain=0x00 payload=0x0000 salt=0x00000000\n");
    assert!(matches!(
        SeedFile::parse(&trailing),
        Err(SeedParseError::StepCountMismatch { .. })
    ));
    let bad_config = replace("members=2", "members=2 extra=1");
    assert!(matches!(
        SeedFile::parse(&bad_config),
        Err(SeedParseError::BadConfig(_))
    ));
    let out_of_range = replace("accounts=1", "accounts=7");
    assert!(matches!(
        SeedFile::parse(&out_of_range),
        Err(SeedParseError::ConfigOutOfRange(_))
    ));
}
