//! Profile-guided optimisation support.
//!
//! An instrumented build (`RUSTFLAGS="-Cprofile-generate=<dir> --cfg azul_pgo"`)
//! writes its counters when the process exits - but the shell ends the process
//! with `std::process::exit` while its other threads still run, and the 25 MB
//! write raced them: every raw profile of libazul.dylib stopped at its last
//! 4 KB buffer and none could be read (2026-10-02). [`dump_profile`] writes the
//! counters NOW, from a quiet point (the `dump_profile` debug op, the e2e
//! runner before it exits), and marks them written so the exit handler does
//! not replace the good file with a truncated one.

#[cfg(azul_pgo)]
extern "C" {
    fn __llvm_profile_dump() -> core::ffi::c_int;
}

/// Writes the PGO counters now, once: later calls and the exit handler write
/// nothing. `false` in a build without `--cfg azul_pgo`, which has no counters.
#[must_use = "false means no profile was written"]
// Not const: the `azul_pgo` build calls the profiler runtime (an extern fn).
#[allow(clippy::missing_const_for_fn)]
pub fn dump_profile() -> bool {
    #[cfg(azul_pgo)]
    {
        // SAFETY: every `-Cprofile-generate` build links the profiler runtime;
        // the call takes no arguments and only writes the counters to the file
        // `LLVM_PROFILE_FILE` names.
        unsafe { __llvm_profile_dump() == 0 }
    }
    #[cfg(not(azul_pgo))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(azul_pgo))]
    #[test]
    fn a_build_without_pgo_has_nothing_to_dump() {
        assert!(!super::dump_profile());
    }
}
