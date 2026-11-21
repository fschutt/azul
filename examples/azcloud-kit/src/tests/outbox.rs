//! The open grant calls: a call keeps its idempotency key across its retries and a restart until
//! a final answer closes it; another request of the same scope gets a key of its own.

use azlin_contract::IdempotentScope;
use azul_storage::testing::TempDir;

use crate::{
    outbox::{is_final, Outbox},
    TokenError,
};

fn refused(status: u16) -> Result<(), TokenError> {
    Err(TokenError::Refused {
        status,
        code: String::from("x"),
        message: String::new(),
    })
}

#[test]
fn an_open_call_keeps_its_key_until_a_final_answer_closes_it() {
    let dir = TempDir::new("azcloud-outbox");
    let outbox = Outbox::new(dir.path().join("outbox"));
    let first = outbox.open(IdempotentScope::Lockdown, "d_1", "f").unwrap();
    assert!(first.kept().is_ok());
    let key = *first.key();
    // a retry, or the app started anew: the same key
    let again = Outbox::new(dir.path().join("outbox"))
        .open(IdempotentScope::Lockdown, "d_1", "f")
        .unwrap();
    assert_eq!(*again.key(), key);
    assert_eq!(outbox.pending(IdempotentScope::Lockdown, "d_1"), Some((key, String::from("f"))));
    // another drive, another scope: calls of their own
    let other = outbox.open(IdempotentScope::Lockdown, "d_2", "f").unwrap();
    assert_ne!(*other.key(), key);
    let claim = outbox.open(IdempotentScope::Claim, "d_1", "").unwrap();
    assert_ne!(*claim.key(), key);
    // closed by its final answer: the next lockdown is a new request
    again.close();
    assert_eq!(outbox.pending(IdempotentScope::Lockdown, "d_1"), None);
    let next = outbox.open(IdempotentScope::Lockdown, "d_1", "f").unwrap();
    assert_ne!(*next.key(), key);
    // a request of the same scope for other words replaces the open one
    let other_words = outbox.open(IdempotentScope::Lockdown, "d_1", "g").unwrap();
    assert_ne!(*other_words.key(), *next.key());
    for call in [first, other, claim, next, other_words] {
        call.close();
    }
}

/// LANE-AZ-2: `files_of` names the file of every scope's open call of a drive: after one call of
/// each scope for two drives, the folder holds exactly the files the two lists name, and what a
/// removal of the first drive takes leaves the second drive's calls open.
#[test]
fn lane_az_2_files_of_names_every_scope_a_drive_has_an_open_call_in() {
    let dir = TempDir::new("azcloud-outbox-files");
    let outbox = Outbox::new(dir.path().join("outbox"));
    let mut open = Vec::new();
    for scope in IdempotentScope::ALL {
        open.push(outbox.open(scope, "d_1", "a call").unwrap());
        open.push(outbox.open(scope, "d_2", "a call").unwrap());
    }
    let mut on_disk: Vec<_> = std::fs::read_dir(outbox.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    on_disk.sort();
    let mut named: Vec<_> = outbox
        .files_of("d_1")
        .into_iter()
        .chain(outbox.files_of("d_2"))
        .collect();
    named.sort();
    assert_eq!(
        on_disk, named,
        "the files there are the files named, no more"
    );

    for file in outbox.files_of("d_1") {
        std::fs::remove_file(file).unwrap();
    }
    for scope in IdempotentScope::ALL {
        assert!(outbox.pending(scope, "d_1").is_none(), "{scope:?}");
        assert!(outbox.pending(scope, "d_2").is_some(), "{scope:?}");
    }
    for call in open {
        call.close();
    }
}

#[test]
fn only_a_final_answer_closes_a_sent_call() {
    let dir = TempDir::new("azcloud-outbox-send");
    let outbox = Outbox::new(dir.path().join("outbox"));
    let mut keys = Vec::new();
    let lost: Result<(), TokenError> = outbox.send(IdempotentScope::Restore, "d_1", "docs/", |key| {
        keys.push(*key);
        Err(TokenError::Connect(String::from("timed out")))
    });
    assert!(lost.is_err());
    let answered = outbox.send(IdempotentScope::Restore, "d_1", "docs/", |key| {
        keys.push(*key);
        Ok(())
    });
    assert!(answered.is_ok());
    assert_eq!(keys[0], keys[1], "the retry carries the lost one's key");
    assert_eq!(outbox.pending(IdempotentScope::Restore, "d_1"), None, "closed");
}

#[test]
fn no_answer_a_cut_answer_a_busy_server_leave_a_call_open() {
    assert!(is_final(&Ok::<(), TokenError>(())));
    assert!(!is_final(&Err::<(), _>(TokenError::Connect(String::new()))));
    assert!(!is_final(&Err::<(), _>(TokenError::Protocol(String::new()))));
    for status in [500, 502, 503, 408, 429] {
        assert!(!is_final(&refused(status)), "{status}");
    }
    for status in [400, 401, 403, 404, 409, 410] {
        assert!(is_final(&refused(status)), "{status}");
    }
    assert!(is_final(&Err::<(), _>(TokenError::SignIn(String::new()))));
}

/// AZ-P20: `Outbox::send` sent when the key could not be written down, whatever the request: after
/// a crash the retry had a key of its own, and a month, a voucher or a checkout could be paid for
/// twice. The requests that spend are not sent without it; a lockdown still is.
#[test]
fn az_p20_a_request_that_spends_is_not_sent_when_its_key_could_not_be_written_down() {
    use std::cell::Cell;

    use crate::outbox::spends;
    let dir = TempDir::new("azcloud-outbox-unwritable");
    // The outbox's folder is under a FILE: no key can be written.
    std::fs::write(dir.path().join("blocked"), b"a file").unwrap();
    let outbox = Outbox::new(dir.path().join("blocked").join("outbox"));
    for scope in IdempotentScope::ALL {
        let sent = Cell::new(false);
        let outcome = outbox.send(scope, "d_1", "words", |_| {
            sent.set(true);
            Ok(())
        });
        if spends(scope) {
            assert!(
                matches!(outcome, Err(TokenError::Config(_))),
                "{scope:?}: {outcome:?}"
            );
            assert!(!sent.get(), "{scope:?} was sent without its key kept");
        } else {
            assert!(outcome.is_ok(), "{scope:?}: {outcome:?}");
            assert!(sent.get(), "{scope:?} is needed most when things go wrong");
        }
    }
    // The ones that spend: money and months, nothing else.
    let spending: Vec<IdempotentScope> = IdempotentScope::ALL
        .into_iter()
        .filter(|scope| spends(*scope))
        .collect();
    assert_eq!(
        spending,
        vec![
            IdempotentScope::Claim,
            IdempotentScope::Redeem,
            IdempotentScope::Voucher,
            IdempotentScope::Issue,
            IdempotentScope::Checkout,
        ]
    );
    // With a folder that takes the key, the same request goes.
    let writable = Outbox::new(dir.path().join("outbox"));
    assert!(writable
        .send(IdempotentScope::Redeem, "d_1", "words", |_| Ok(()))
        .is_ok());
}
