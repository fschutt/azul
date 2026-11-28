//! What a read of an AAudio input stream means. No device.

use super::*;

/// A read that returned no frames is not the end of the microphone: the first buffer of a started stream takes
/// longer than the 100 ms read. The reader retries for 5 s, then gives the stream up.
///
/// Mutation: `rc == 0` answering `Action::Stop` (the state of 46d6ad5ffd: a slow start ended the microphone for good).
#[test]
fn an_empty_read_is_waited_out_for_a_while_not_the_end() {
    let mut patience = Patience::new();
    for read in 1..MAX_EMPTY_READS {
        assert_eq!(patience.next(0), Action::Retry, "empty read {read}");
    }
    assert_eq!(patience.next(0), Action::Stop, "5 s of silence is a dead stream");
}

/// Data ends the waiting: the count starts again at the next silence.
#[test]
fn data_restores_the_patience() {
    let mut patience = Patience::new();
    for _ in 0..MAX_EMPTY_READS - 1 {
        assert_eq!(patience.next(0), Action::Retry);
    }
    assert_eq!(patience.next(480), Action::Deliver(480));
    for _ in 0..MAX_EMPTY_READS - 1 {
        assert_eq!(patience.next(0), Action::Retry);
    }
    assert_eq!(patience.next(0), Action::Stop);
}

/// A stream the system took away (a headset plugged in, another app took the microphone) is opened again, a few
/// times; any other error ends the microphone.
///
/// Mutation: `ERROR_DISCONNECTED` answering `Action::Stop` (a headset plug ended the microphone tile for good).
#[test]
fn a_disconnected_stream_is_opened_again_a_few_times() {
    let mut patience = Patience::new();
    for _ in 0..MAX_REOPENS {
        assert_eq!(patience.next(ERROR_DISCONNECTED), Action::Reopen);
        assert_eq!(patience.next(256), Action::Deliver(256), "it delivers again after the reopen");
    }
    assert_eq!(patience.next(ERROR_DISCONNECTED), Action::Stop, "a stream that keeps going away is given up");
}

/// An error that is not a disconnect (invalid state, internal, unavailable, a null stream) is the end.
#[test]
fn any_other_error_ends_the_microphone() {
    for rc in [-895, -896, -889, -886, -1] {
        let mut patience = Patience::new();
        assert_eq!(patience.next(rc), Action::Stop, "rc {rc}");
    }
}
