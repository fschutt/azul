    #![allow(dead_code, unused_imports)]
    //! Rust wrappers for `Instant` / `Duration` classes
    use crate::dll::*;
    use std::ffi::c_void;


    /// `Instant` struct
    #[doc(inline)] pub use crate::dll::AzInstantPtr as Instant;

    impl Instant {
        /// Creates a new `Instant` instance.
        pub fn now() -> Self { (crate::dll::get_azul_dll().az_instant_ptr_now)() }
    }

    impl Drop for Instant { fn drop(&mut self) { (crate::dll::get_azul_dll().az_instant_ptr_delete)(self); } }


    /// `Duration` struct
    #[doc(inline)] pub use crate::dll::AzDuration as Duration;

    impl Clone for Duration { fn clone(&self) -> Self { *self } }
    impl Copy for Duration { }
