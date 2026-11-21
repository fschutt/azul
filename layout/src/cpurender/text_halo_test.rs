//! Tests of who owns the kept text halos: a [`TextHaloStore`] of its own for every window, so that
//! nothing one store draws, counts, bounds, switches or gives back reaches another, and so that the
//! code of a scan may ask the store it runs in.

use super::*;

/// The bytes of a picture of 4 x 4 px.
const BYTES: usize = 4 * 4 * 4;
/// The geometry the labels of these tests share.
const GEOMETRY: [i32; 3] = [0, 300, 1836];

/// A picture of 4 x 4 px, its ink all of it.
fn picture() -> Kept {
    Kept {
        image: ImageRef::null_image(4, 4, RawImageFormat::RGBA8, Vec::new()),
        offset: (0, 0),
        width: 4,
        height: 4,
        scale: 24.0,
        ink: (0, 0, 4, 4),
        reach: 0,
        slot: 0,
    }
}

/// What label `n` is found by: a text and a shape of its own.
fn label(n: u64) -> Sought {
    Sought {
        text: n,
        shape: 1_000_000 + n,
        anchor: (0, 0),
        scale: 24.0,
    }
}

/// A window is reached from the renderer host's thread and from its callbacks: so is its store.
///
/// Mutation: a `Rc` or a `RefCell` among the store's fields fails to compile here.
#[test]
fn a_store_may_be_shared_between_threads() {
    fn shared<T: Send + Sync>() {}
    shared::<TextHaloStore>();
}

#[test]
fn a_new_store_has_kept_and_drawn_nothing() {
    let store = TextHaloStore::default();
    assert_eq!(store.text_halo_stats(), TextHaloStats::default());
    assert_eq!(store.halos_drawn(), 0);
    assert!(!store.halo_scan_changed_pictures());
    assert!(format!("{store:?}").contains("TextHaloStore"));
}

/// What one store draws, counts, bounds, switches and gives back is its own: two windows of one
/// thread kept these in one place, and the second one found the first one's halos, numbers and
/// switches.
///
/// Mutation: the state of `TextHaloStore::new` made once and shared (a `static`, a thread-local)
/// fails the first block.
#[test]
fn two_stores_share_nothing() {
    let (a, b) = (TextHaloStore::new(), TextHaloStore::new());

    // A scan of a: one halo as of right, none after it, and one picture kept.
    let ((), left) = a.within_halo_budget(1, Duration::ZERO, || {
        assert!(a.admit(BYTES, 1), "the one halo of right");
        assert!(!a.admit(BYTES, 2), "no time for a second");
        a.with_halos(|cache| cache.insert(&label(1), &GEOMETRY, Some(picture()), BYTES, false));
    });
    assert_eq!(left, 1);
    a.text_halo_bounds(5, 10 * BYTES);
    a.text_halo_slots(false);
    a.draw_every_small_halo_in_the_next_scan();
    // b holds a picture of the same label.
    b.with_halos(|cache| cache.insert(&label(1), &GEOMETRY, Some(picture()), BYTES, false));
    b.text_halo_slots(true);

    // Nothing of a's scan, bounds, switches or flag is b's.
    let stats = b.text_halo_stats();
    assert_eq!((stats.scans, stats.pending, stats.created_total), (0, 0, 1), "{stats:?}");
    assert_eq!(b.halos_drawn(), 0);
    assert_eq!(
        b.with_halos(|cache| (cache.max_entries, cache.max_bytes)),
        (MAX_HALOS, MAX_HALO_BYTES)
    );
    assert!(!a.slots_on() && b.slots_on(), "each store has its own answer");
    let ((), _) = b.within_halo_budget(0, Duration::ZERO, || {
        assert!(!b.every_small(), "a's next scan is a's");
    });

    // And a's are still a's: its count, its flag for the next scan, its bound.
    assert_eq!(a.halos_drawn(), 1);
    let ((), _) = a.within_halo_budget(0, Duration::ZERO, || {
        assert!(a.every_small(), "a's next scan draws every small picture");
    });
    assert_eq!(a.with_halos(|cache| cache.max_entries), 5);

    // A release of a gives back a's picture and leaves b's.
    a.text_halos_forget_use();
    assert_eq!(
        a.release_text_halos(true),
        TextHalosReleased {
            halos: 1,
            bytes: BYTES
        }
    );
    assert_eq!(a.text_halo_stats().kept, 0);
    assert_eq!(b.text_halo_stats().kept, 1, "a's release gave back nothing of b's");
}

/// A scan may be asked inside a scan: the budget of the outer one is back when the inner one ends.
#[test]
fn a_scan_inside_a_scan_gives_the_outer_one_its_budget_back() {
    let store = TextHaloStore::new();
    let ((), outer_left) = store.within_halo_budget(1, Duration::ZERO, || {
        assert!(store.admit(BYTES, 1), "the outer scan's one halo");
        let ((), inner_left) = store.within_halo_budget(0, Duration::ZERO, || {
            assert!(!store.admit(BYTES, 2), "the inner scan has none to draw");
        });
        assert_eq!(inner_left, 1);
        assert!(!store.admit(BYTES, 3), "the outer scan's budget is back, and it is spent");
    });
    assert_eq!(outer_left, 1);
}

/// The code of a scan asks the store for the halos of the lists: the lock is not held while it
/// runs. A release is asked for in vain meanwhile (the scan is using what it has looked at).
#[test]
fn the_closure_of_a_scan_may_ask_the_store() {
    let store = TextHaloStore::new();
    let (seen, _) = store.within_halo_budget(2, Duration::ZERO, || {
        assert!(store.text_halos(&[], 2.0, None).is_empty());
        let released = store.release_text_halos(true);
        (store.text_halo_stats().scans, store.halos_drawn(), released)
    });
    assert_eq!(seen, (1, 0, TextHalosReleased::default()));
}

/// One store asked from two threads: every scan is counted, and none gets stuck behind another.
#[test]
fn a_store_asked_from_two_threads_counts_every_scan() {
    const SCANS: u64 = 50;
    let store = TextHaloStore::new();
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                for _ in 0..SCANS {
                    let _ = store.within_halo_budget(1, Duration::ZERO, || store.admit(BYTES, 1));
                }
            });
        }
    });
    assert_eq!(store.text_halo_stats().scans, 2 * SCANS);
}
