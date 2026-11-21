//! C1 (the recovery code's text) and C2 (the written-down quiz) of lane W1-S15d.
//!
//! Implements planning/architecture.md section 5.5 ("at setup the app checks that the recovery code
//! was written down by asking for 4 of its 5 groups") and section 7.4 (the paper key). The known
//! answers are `code_*` rows of `tests/vectors/contacts.json`, made by the independent program of
//! the lane.

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code, clippy::needless_range_loop)]

mod recovery_common;

use azlin_keys::recovery::{Quiz, QuizFailure, RecoveryCode, RecoveryError};
use azlin_keys::{EntropyError, FailingEntropy, Secret, SeededEntropy};
use recovery_common::vectors;
use recovery_common::{from_hex, input, pat, pat16, to_hex, word, ScriptedEntropy, SplitMix64};

const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn code_of(bytes: [u8; 16]) -> RecoveryCode {
    RecoveryCode::from_secret(Secret::from_bytes(bytes))
}

/// The 26 symbols of the code without hyphens.
fn symbols(code: &RecoveryCode) -> String {
    code.to_text().chars().filter(|c| *c != '-').collect()
}

// ---------------------------------------------------------------------------------------------
// C1
// ---------------------------------------------------------------------------------------------

/// C1: generate and parse round trip, the groups 5/5/5/5/6, the Crockford alphabet, the known
/// answers.
#[test]
fn c1_recovery_code_text() {
    let mut ent = SeededEntropy::from_u64(2026);
    for i in 0..300 {
        let code = RecoveryCode::generate(&mut ent).expect("a code");
        let text = code.to_text();
        assert_eq!(text.len(), 30, "code {i}");
        let groups: Vec<usize> = text.split('-').map(str::len).collect();
        assert_eq!(groups, [5, 5, 5, 5, 6], "code {i}");
        assert!(
            text.chars().all(|c| c == '-' || ALPHABET.contains(c)),
            "code {i}: {}",
            text.as_str()
        );
        let back = RecoveryCode::parse(&text).expect("the text parses");
        assert!(back.ct_eq(&code), "code {i}");
        assert_eq!(back.check_value(), code.check_value());
    }

    // The known answers: every `code_*` row of the vector file.
    let rows = vectors::read("contacts.json");
    let mut seen = 0;
    for r in rows.iter().filter(|r| r.name.starts_with("code_")) {
        let text = String::from_utf8(from_hex(&r.hex)).expect("an ASCII text");
        if r.valid {
            let code = RecoveryCode::parse(&text).unwrap_or_else(|e| panic!("{}: {e:?}", r.name));
            assert_eq!(code.to_text().as_str(), r.expect, "{}", r.name);
            if r.inputs.contains("bytes=") {
                let bytes = from_hex(input(&r.inputs, "bytes"));
                let made = code_of(<[u8; 16]>::try_from(bytes.as_slice()).expect("16 bytes"));
                assert_eq!(
                    made.to_text().as_str(),
                    r.expect,
                    "{}: from the bytes",
                    r.name
                );
                assert!(code.ct_eq(&made), "{}", r.name);
                assert_eq!(
                    to_hex(&code.check_value()),
                    input(&r.inputs, "check_value"),
                    "{}: the check value",
                    r.name
                );
            }
        } else {
            let error = RecoveryCode::parse(&text).expect_err(&r.name);
            assert_eq!(word(&error), r.expect, "{}", r.name);
        }
        seen += 1;
    }
    assert!(seen >= 15, "only {seen} code rows in the vector file");
}

/// C1: a person types the code back in any case, with spaces, hyphens or line breaks between the
/// symbols, `O` for `0` and `I` or `L` for `1`.
#[test]
fn c1_the_text_is_read_as_people_type_it() {
    let code = code_of(pat16(0x10));
    let text = code.to_text();
    let compact = symbols(&code);
    assert_eq!(text.as_str(), "208H4-4RM2M-B1E60-S38DH-R78Y3W");
    let one_by_one: String = compact.chars().map(|c| format!("{c} ")).collect();
    for variant in [
        compact.clone(),
        compact.to_lowercase(),
        text.replace('-', " "),
        format!("  {}\n", text.as_str()),
        text.replace('-', "--"),
        text.replace('-', "\n"),
        text.replace('-', "\t"),
        text.replace('-', "\u{a0}"),
        text.replace('-', "\u{3000}"),
        one_by_one,
        text.to_lowercase().replace('-', " "),
        compact.replace('0', "O").replace('1', "I"),
        compact.replace('0', "o").replace('1', "l"),
        compact.replace('1', "L"),
    ] {
        let parsed = RecoveryCode::parse(&variant).unwrap_or_else(|e| panic!("{variant:?}: {e:?}"));
        assert!(parsed.ct_eq(&code), "{variant:?}");
    }
}

/// C1: a text of 25 or 27 symbols, an empty one, `U`, other characters, non-ASCII letters and a
/// padding bit that is not zero are refused, each with its own word.
#[test]
fn c1_wrong_texts_are_refused_with_their_word() {
    let code = code_of(pat16(0x10));
    let compact = symbols(&code);
    let refuse = |text: &str| RecoveryCode::parse(text).expect_err(text);
    assert_eq!(refuse(&compact[..25]), RecoveryError::CodeLength);
    assert_eq!(refuse(&format!("{compact}0")), RecoveryError::CodeLength);
    assert_eq!(
        refuse(&format!("{compact}{compact}")),
        RecoveryError::CodeLength
    );
    assert_eq!(refuse(""), RecoveryError::CodeLength);
    assert_eq!(refuse("-- \n"), RecoveryError::CodeLength);
    assert_eq!(refuse(&compact[..1]), RecoveryError::CodeLength);
    for bad in [
        'U', 'u', '!', '/', ':', 'é', 'ø', '€', '٣', '\u{ff10}', '_', '+',
    ] {
        let text = format!("{bad}{}", &compact[1..]);
        assert_eq!(refuse(&text), RecoveryError::CodeCharacter, "{bad:?}");
        let tail = format!("{}{bad}", &compact[..25]);
        assert_eq!(refuse(&tail), RecoveryError::CodeCharacter, "{bad:?} last");
    }
    // The last symbol carries three bits of the code and two zero padding bits.
    let last = compact.chars().next_back().expect("a symbol");
    let value = ALPHABET.find(last).expect("in the alphabet");
    assert!(
        value.is_multiple_of(4),
        "the last symbol of a printed code has zero padding"
    );
    for set in 1..4usize {
        let wrong = ALPHABET.chars().nth(value + set).expect("a symbol");
        let text = format!("{}{wrong}", &compact[..25]);
        assert_eq!(refuse(&text), RecoveryError::CodePadding, "{set}");
    }
    // Too long is refused as too long even when a symbol is wrong too: the length comes first.
    assert_eq!(
        refuse(&format!("{compact}UU")),
        RecoveryError::CodeLength,
        "27th symbol"
    );
}

/// C1: every symbol value in every position of the text is read where it is written: the bits of
/// the code are those of the number the text spells (an independent computation), and the padding
/// bits of the last symbol must be zero.
#[test]
fn c1_every_symbol_in_every_position_is_read_where_it_is_written() {
    for position in 0..26usize {
        for (value, symbol) in ALPHABET.chars().enumerate() {
            let mut text = ['0'; 26];
            text[position] = symbol;
            let text: String = text.iter().collect();
            let padding_zero = position != 25 || value.is_multiple_of(4);
            match RecoveryCode::parse(&text) {
                Ok(code) => {
                    assert!(padding_zero, "{text}: the padding bits are not zero");
                    // The 130-bit number of the text, without its two padding bits: the code.
                    let expected: u128 = if position < 25 {
                        (value as u128) << (123 - 5 * position)
                    } else {
                        (value as u128) >> 2
                    };
                    assert_eq!(code.as_secret().expose(), &expected.to_be_bytes(), "{text}");
                    assert_eq!(symbols(&code), text, "{text}: printed back");
                }
                Err(error) => {
                    assert!(!padding_zero, "{text}: {error:?}");
                    assert_eq!(error, RecoveryError::CodePadding, "{text}");
                }
            }
        }
    }
}

/// C1: the check value is four bytes, differs for other codes, and is the same for the same code
/// however it was made.
#[test]
fn c1_the_check_value_names_the_code_in_four_bytes() {
    let a = code_of(pat16(0x10));
    assert_eq!(a.check_value().len(), 4);
    assert_eq!(a.check_value(), code_of(pat16(0x10)).check_value());
    let mut ent = SeededEntropy::from_u64(11);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..500 {
        let code = RecoveryCode::generate(&mut ent).expect("a code");
        seen.insert(code.check_value());
    }
    // 500 codes, 2^32 values: a collision is a one-in-ten-thousand event, not a pattern.
    assert!(
        seen.len() >= 499,
        "{} distinct check values of 500 codes",
        seen.len()
    );
    // One bit of the code changes it.
    let mut bytes = pat16(0x10);
    bytes[7] ^= 0x04;
    assert_ne!(code_of(bytes).check_value(), a.check_value());
}

// ---------------------------------------------------------------------------------------------
// C2 [M]
// ---------------------------------------------------------------------------------------------

/// The answers a person writes down from the code's text, for the groups `quiz` asks.
fn right_answers(code: &RecoveryCode, quiz: &Quiz) -> [String; 4] {
    let text = code.to_text();
    let groups: Vec<&str> = text.split('-').collect();
    quiz.positions()
        .map(|p| groups[usize::from(p) - 1].to_string())
}

fn check(code: &RecoveryCode, quiz: &Quiz, answers: &[String; 4]) -> Result<(), QuizFailure> {
    let refs = [
        answers[0].as_str(),
        answers[1].as_str(),
        answers[2].as_str(),
        answers[3].as_str(),
    ];
    quiz.check(code, &refs)
}

/// C2: the quiz leaves out one group, chosen from the seam; the other four are asked, in ascending
/// order, from 1 to 5. A byte below 255 chooses (modulo 5), a byte of 255 is drawn again.
#[test]
fn c2_quiz_positions_come_from_the_seam() {
    for (byte, left_out) in [
        (0u8, 1u8),
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 1),
        (9, 5),
        (254, 5),
        (100, 1),
    ] {
        let mut ent = ScriptedEntropy::new(vec![byte]);
        let quiz = Quiz::new(&mut ent).expect("a quiz");
        let want: Vec<u8> = (1..=5u8).filter(|g| *g != left_out).collect();
        assert_eq!(quiz.positions().to_vec(), want, "byte {byte}");
        assert_eq!(ent.draws, [1], "one byte is drawn");
    }
    // 255 is not used (it would favour group 1): the next byte is drawn.
    let mut ent = ScriptedEntropy::new(vec![255, 255, 3]);
    let quiz = Quiz::new(&mut ent).expect("a quiz");
    assert_eq!(quiz.positions(), [1, 2, 3, 5]);
    assert_eq!(ent.draws, [1, 1, 1]);
    // The same seed gives the same quiz.
    for seed in 0..50u64 {
        let a = Quiz::new(&mut SeededEntropy::from_u64(seed)).expect("a quiz");
        let b = Quiz::new(&mut SeededEntropy::from_u64(seed)).expect("a quiz");
        assert_eq!(a, b, "seed {seed}");
    }
}

/// C2: four distinct positions of five, ascending; every group is left out about as often as the
/// others over 2,000 seeds.
#[test]
fn c2_quiz_positions_are_four_distinct_of_five_and_each_group_can_be_left_out() {
    let mut left_out = [0u32; 5];
    for seed in 0..2_000u64 {
        let quiz = Quiz::new(&mut SeededEntropy::from_u64(seed)).expect("a quiz");
        let positions = quiz.positions();
        assert!(
            positions.iter().all(|p| (1..=5).contains(p)),
            "{positions:?}"
        );
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
        let missing = (1..=5u8)
            .find(|g| !positions.contains(g))
            .expect("one is left out");
        left_out[usize::from(missing) - 1] += 1;
    }
    for (i, count) in left_out.iter().enumerate() {
        assert!(
            (300..=500).contains(count),
            "group {} was left out {count} times of 2,000: {left_out:?}",
            i + 1
        );
    }
}

/// C2: a source that fails, or never gives a usable byte, makes no quiz.
#[test]
fn c2_quiz_makes_nothing_from_a_failing_source() {
    assert_eq!(
        Quiz::new(&mut FailingEntropy::after(0)),
        Err(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    assert_eq!(
        Quiz::new(&mut ScriptedEntropy::new(Vec::new())),
        Err(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    // 64 draws are the limit: the 64th byte may still be the usable one, a 65th is not drawn.
    let mut sixty_three = vec![255u8; 63];
    sixty_three.push(3);
    let mut ent = ScriptedEntropy::new(sixty_three);
    assert_eq!(
        Quiz::new(&mut ent).expect("a quiz").positions(),
        [1, 2, 3, 5]
    );
    assert_eq!(ent.draws.len(), 64);
    let mut sixty_four = vec![255u8; 64];
    sixty_four.push(3);
    let mut ent = ScriptedEntropy::new(sixty_four);
    assert!(Quiz::new(&mut ent).is_err());
    assert_eq!(ent.draws.len(), 64, "the limit is 64 draws");
}

/// C2 [M]: the right answers pass, however they are typed.
#[test]
fn c2_quiz_right_answers_pass() {
    let mut ent = SeededEntropy::from_u64(8);
    for _ in 0..40 {
        let code = RecoveryCode::generate(&mut ent).expect("a code");
        let quiz = Quiz::new(&mut ent).expect("a quiz");
        let answers = right_answers(&code, &quiz);
        assert_eq!(check(&code, &quiz, &answers), Ok(()));
        // Lower case, spaces and hyphens inside, O for 0, I and L for 1.
        let sloppy = answers.clone().map(|a| {
            a.to_lowercase()
                .replace('0', "o")
                .replace('1', "l")
                .chars()
                .map(|c| format!("{c} "))
                .collect::<String>()
        });
        assert_eq!(check(&code, &quiz, &sloppy), Ok(()), "{sloppy:?}");
        let dashed = answers.clone().map(|a| {
            a.replace('1', "I")
                .chars()
                .map(|c| format!("-{c}"))
                .collect::<String>()
        });
        assert_eq!(check(&code, &quiz, &dashed), Ok(()), "{dashed:?}");
    }
}

/// C2 [M]: one wrong group, a swapped pair, answers for another code, a missing answer, an answer
/// of the left-out group and a mistyped symbol all fail, naming the first wrong group.
#[test]
fn c2_quiz_wrong_answers_fail_and_name_the_first_wrong_group() {
    let code = code_of(pat16(0x10));
    let other = code_of(pat16(0x77));
    let quiz = Quiz::new(&mut ScriptedEntropy::new(vec![2])).expect("a quiz");
    assert_eq!(quiz.positions(), [1, 2, 4, 5]);
    let right = right_answers(&code, &quiz);
    assert_eq!(right.clone().map(|a| a.len()), [5, 5, 5, 6]);
    assert_eq!(check(&code, &quiz, &right), Ok(()));

    // One wrong group, in each of the four places: it is the one named.
    for i in 0..4 {
        let mut answers = right.clone();
        let mut chars: Vec<char> = answers[i].chars().collect();
        let first = chars[0];
        chars[0] = if first == 'Z' { 'Y' } else { 'Z' };
        answers[i] = chars.into_iter().collect();
        assert_eq!(
            check(&code, &quiz, &answers),
            Err(QuizFailure::Wrong(quiz.positions()[i])),
            "place {i}"
        );
    }
    // A mistyped LAST symbol of a group.
    let mut answers = right.clone();
    answers[3].pop();
    answers[3].push('Z');
    assert_eq!(check(&code, &quiz, &answers), Err(QuizFailure::Wrong(5)));
    // A swapped pair names the first of the two places.
    for (i, j) in [(0, 1), (1, 2), (0, 3), (2, 3)] {
        let mut answers = right.clone();
        answers.swap(i, j);
        assert_eq!(
            check(&code, &quiz, &answers),
            Err(QuizFailure::Wrong(quiz.positions()[i])),
            "{i} and {j} swapped"
        );
    }
    // Two wrong groups: the first of them is named.
    let mut answers = right.clone();
    answers[1] = "ZZZZZ".to_string();
    answers[3] = "ZZZZZZ".to_string();
    assert_eq!(check(&code, &quiz, &answers), Err(QuizFailure::Wrong(2)));
    // Answers for another code.
    let others = right_answers(&other, &quiz);
    assert_eq!(check(&code, &quiz, &others), Err(QuizFailure::Wrong(1)));
    assert_eq!(check(&other, &quiz, &right), Err(QuizFailure::Wrong(1)));
    // A group too short, too long, empty, or with a letter Crockford does not have.
    for bad in [
        "", "0", "2", "208H", "208H4X", "208H44", "U08H4", "!!!!!", "      ",
    ] {
        let mut answers = right.clone();
        answers[0] = bad.to_string();
        assert_eq!(
            check(&code, &quiz, &answers),
            Err(QuizFailure::Wrong(1)),
            "{bad:?}"
        );
    }
    // The group that was LEFT OUT is not asked: giving it in a place does not pass.
    let text = code.to_text();
    let groups: Vec<&str> = text.split('-').collect();
    let mut answers = right.clone();
    answers[2] = groups[2].to_string();
    assert_eq!(check(&code, &quiz, &answers), Err(QuizFailure::Wrong(4)));
    // A very long answer is wrong without being read to the end.
    let mut answers = right.clone();
    answers[0] = "0".repeat(10_000);
    assert_eq!(check(&code, &quiz, &answers), Err(QuizFailure::Wrong(1)));
    let mut answers = right.clone();
    answers[0] = format!("{} {}", right[0], "0".repeat(100));
    assert_eq!(check(&code, &quiz, &answers), Err(QuizFailure::Wrong(1)));
}

/// C2 [M]: every position of every quiz is compared: a wrong group in any of the four places of
/// any of the five quizzes fails.
#[test]
fn c2_quiz_every_asked_group_is_compared() {
    let code = code_of(pat16(0x31));
    for byte in 0..5u8 {
        let quiz = Quiz::new(&mut ScriptedEntropy::new(vec![byte])).expect("a quiz");
        let right = right_answers(&code, &quiz);
        assert_eq!(check(&code, &quiz, &right), Ok(()));
        for place in 0..4 {
            // Change each symbol of the group in turn: all are compared.
            for symbol_index in 0..right[place].len() {
                let mut chars: Vec<char> = right[place].chars().collect();
                chars[symbol_index] = if chars[symbol_index] == '7' { '8' } else { '7' };
                let mut answers = right.clone();
                answers[place] = chars.into_iter().collect();
                assert_eq!(
                    check(&code, &quiz, &answers),
                    Err(QuizFailure::Wrong(quiz.positions()[place])),
                    "quiz {byte}, place {place}, symbol {symbol_index}"
                );
            }
        }
    }
}
