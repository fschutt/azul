    #![allow(dead_code, unused_imports)]
    //! Rust wrappers for `Instant` / `Duration` classes
    use crate::dll::*;
    use std::ffi::c_void;


    /// `Instant` struct
    #[doc(inline)] pub use crate::dll::AzInstantPtr as Instant;

    impl Instant {
        /// Creates a new `Instant` instance.
        pub fn now() -> Self { unsafe { crate::dll::az_instant_ptr_now() } }
    }

    impl Drop for Instant { fn drop(&mut self) { unsafe { crate::dll::az_instant_ptr_delete(self) }; } }


    /// `Duration` struct
    #[doc(inline)] pub use crate::dll::AzDuration as Duration;

    impl Duration {
        /// Creates a new `Duration` instance.
        pub fn milliseconds(milliseconds: usize) -> Self { unsafe { crate::dll::az_duration_milliseconds(milliseconds) } }
        /// Creates a new `Duration` instance.
        pub fn seconds(seconds: usize) -> Self { unsafe { crate::dll::az_duration_seconds(seconds) } }
    }

    impl Clone for Duration { fn clone(&self) -> Self { *self } }
    impl Copy for Duration { }
