use super::*;

#[test]
fn items_come_back_oldest_first_and_only_once() {
    let queue = SharedQueue::new();
    queue.push(1);
    queue.push(2);
    queue.push(3);
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.drain(), vec![1, 2, 3]);
    assert!(queue.is_empty());
    assert!(queue.drain().is_empty());
}

#[test]
fn a_clone_is_the_same_queue_and_a_new_queue_is_not() {
    let queue = SharedQueue::new();
    let clone = queue.clone();
    let other = SharedQueue::new();
    clone.push("from the clone");
    assert!(queue.is_shared_with(&clone));
    assert!(!queue.is_shared_with(&other));
    assert_eq!(queue.drain(), vec!["from the clone"]);
    assert!(other.is_empty(), "another queue never sees the items");
}

#[test]
fn the_bound_hands_the_item_back() {
    let queue = SharedQueue::new();
    assert_eq!(queue.push_up_to('a', 2), Ok(()));
    assert_eq!(queue.push_up_to('b', 2), Ok(()));
    assert_eq!(queue.push_up_to('c', 2), Err('c'));
    assert_eq!(queue.drain(), vec!['a', 'b']);
}

#[test]
fn draining_up_to_leaves_the_newer_items() {
    let queue = SharedQueue::new();
    for n in 0..5 {
        queue.push(n);
    }
    assert_eq!(queue.drain_up_to(2), vec![0, 1]);
    assert_eq!(queue.drain_up_to(99), vec![2, 3, 4]);
    assert!(queue.drain_up_to(1).is_empty());
}

#[test]
fn draining_matching_keeps_the_others_in_order() {
    let queue = SharedQueue::new();
    for n in 0..6 {
        queue.push(n);
    }
    assert_eq!(queue.drain_matching(|n| n % 2 == 0), vec![0, 2, 4]);
    assert_eq!(queue.drain(), vec![1, 3, 5]);
}

#[test]
fn queues_compare_equal_whatever_they_hold() {
    let empty: SharedQueue<u8> = SharedQueue::new();
    let full = SharedQueue::new();
    full.push(7);
    assert_eq!(empty, full, "a queue is a connection, not state");
}

#[test]
fn a_queue_a_thread_pushed_into_is_drained_here() {
    let queue = SharedQueue::new();
    let sink = queue.clone();
    let worker = std::thread::spawn(move || {
        for n in 0..100 {
            sink.push(n);
        }
    });
    worker.join().expect("the worker ends");
    assert_eq!(queue.drain(), (0..100).collect::<Vec<_>>());
}

#[test]
fn a_poisoned_lock_is_recovered() {
    let queue: SharedQueue<u8> = SharedQueue::new();
    let held = queue.clone();
    let poisoner = std::thread::spawn(move || {
        let _guard = held.items.lock().expect("not poisoned yet");
        panic!("poison the lock");
    });
    assert!(poisoner.join().is_err());
    queue.push(9);
    assert_eq!(queue.drain(), vec![9]);
}

#[test]
fn a_unique_push_leaves_an_equal_item_alone() {
    let queue = SharedQueue::new();
    assert!(queue.push_unique_up_to(1, 8));
    assert!(!queue.push_unique_up_to(1, 8), "already waiting");
    assert!(queue.push_unique_up_to(2, 8));
    assert_eq!(queue.drain(), vec![1, 2]);
    assert!(queue.push_unique_up_to(1, 8), "a drained item may come again");
}

#[test]
fn a_unique_push_stops_at_the_bound() {
    let queue = SharedQueue::new();
    assert!(queue.push_unique_up_to('a', 2));
    assert!(queue.push_unique_up_to('b', 2));
    assert!(!queue.push_unique_up_to('c', 2), "the queue is full");
}

#[test]
fn taking_the_first_match_leaves_the_rest() {
    let queue = SharedQueue::new();
    for n in [5, 6, 7, 6] {
        queue.push(n);
    }
    assert_eq!(queue.take_first(|n| *n == 6), Some(6));
    assert_eq!(queue.take_first(|n| *n == 9), None);
    assert_eq!(queue.drain(), vec![5, 7, 6]);
}
