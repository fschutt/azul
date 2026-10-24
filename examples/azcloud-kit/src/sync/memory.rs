//! How much memory this process took at most (its peak resident set), so a test can bound what
//! a pass over a burst of files costs: a sync must stream, never hold a folder in memory.

/// The peak resident set of this process so far, in bytes; `None` where the system does not say.
#[must_use]
pub fn peak_bytes() -> Option<u64> {
    #[cfg(unix)]
    {
        // SAFETY: getrusage only writes the struct it is given.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
            return None;
        }
        let max = u64::try_from(usage.ru_maxrss).ok()?;
        // macOS counts bytes, the others kilobytes.
        Some(if cfg!(target_os = "macos") { max } else { max * 1024 })
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_peak_grows_with_what_the_process_holds() {
        let Some(before) = super::peak_bytes() else {
            return;
        };
        let held = vec![1u8; 64 << 20];
        let after = super::peak_bytes().unwrap_or(0);
        assert!(after >= before);
        assert!(after >= 64 << 20, "{after} bytes, holding {}", held.len());
    }
}
