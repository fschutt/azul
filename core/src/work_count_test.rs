#[allow(unused_imports)]
pub use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_starts_at_nothing_and_adds_up() {
        let work = WorkCount::new();
        assert_eq!(work.get(), 0);
        work.bump();
        work.add(41);
        assert_eq!(work.get(), 42);
        work.reset();
        assert_eq!(work.get(), 0);
    }

    /// A clone starts where what it copies stood, and the two count apart from then on.
    #[test]
    fn a_clone_takes_the_count_of_its_original_and_counts_apart() {
        let original = WorkCount::new();
        original.add(5);
        let copy = original.clone();
        assert_eq!(copy.get(), 5);
        copy.bump();
        assert_eq!((original.get(), copy.get()), (5, 6));
    }

    /// The count is no part of what a struct is: structs that differ in nothing else are equal.
    #[test]
    fn counts_never_make_two_structs_unequal() {
        #[derive(PartialEq, Eq, Debug)]
        struct Counted {
            content: u32,
            work: WorkCount,
        }
        let (a, b) = (
            Counted { content: 1, work: WorkCount::new() },
            Counted { content: 1, work: WorkCount::new() },
        );
        b.work.add(1000);
        assert_eq!(a, b);
        assert_ne!(a, Counted { content: 2, work: WorkCount::new() });
    }

    /// Two threads count into one without losing a step.
    #[test]
    fn counting_from_two_threads_loses_nothing() {
        let work = WorkCount::new();
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let _handle = scope.spawn(|| {
                    for _ in 0..10_000 {
                        work.bump();
                    }
                });
            }
        });
        assert_eq!(work.get(), 20_000);
    }
}
