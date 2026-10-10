//! One bridge per user.

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    /// A second `serve` of the same state folder is refused and names the process that serves
    /// and its ports; once that one ends, the lock is free (the OS drops it with its process, so
    /// a crash leaves nothing to clear).
    #[test]
    fn a_second_serve_is_refused_and_names_the_one_running() {
        let dir = TempDir::new("bridge-single");
        let first = claim(&dir.0).expect("the first serve");
        first.announce(&Running {
            pid: 4242,
            imap: 1143,
            smtp: 1025,
            dav: 1180,
            pim: 1181,
            started: 1_790_843_400,
        });
        let second = claim(&dir.0).err().expect("the second is refused");
        assert!(second.contains("4242") && second.contains("1143"), "{second}");
        assert_eq!(running(&dir.0).map(|r| r.pid), Some(4242));
        drop(first);
        assert_eq!(running(&dir.0), None, "nobody serves");
        assert!(claim(&dir.0).is_ok(), "free once the first ends");
    }
}
