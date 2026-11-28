use super::*;

#[test]
fn a_backlog_counts_chunks_in_and_out_up_to_its_cap() {
    let backlog = Backlog::default();
    assert_eq!(backlog.len(), 0);
    for _ in 0..3 {
        assert!(backlog.try_add(3));
    }
    assert_eq!(backlog.len(), 3);
    assert!(!backlog.try_add(3), "the fourth chunk is refused");
    assert_eq!(backlog.len(), 3, "a refused chunk is not counted");
    backlog.done();
    assert!(backlog.try_add(3), "one decoded makes room for one");
    for _ in 0..10 {
        backlog.done();
    }
    assert_eq!(backlog.len(), 0, "never below nothing");
}

#[test]
fn the_codec_thread_and_the_handle_share_one_count() {
    let handle = Backlog::default();
    let thread = handle.clone();
    assert!(handle.try_add(2));
    assert!(handle.try_add(2));
    assert!(!handle.try_add(2));
    let worker = std::thread::spawn(move || {
        thread.done();
        thread.done();
    });
    worker.join().expect("the thread ends");
    assert_eq!(handle.len(), 0);
    assert!(handle.try_add(2));
}

#[test]
fn the_cap_is_seventeen_seconds_of_thirty_frames_a_second() {
    assert_eq!(DECODE_QUEUE_CHUNKS, 512);
    assert!(DECODE_QUEUE_CHUNKS / 30 >= 17);
}
