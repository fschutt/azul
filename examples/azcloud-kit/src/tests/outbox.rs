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
