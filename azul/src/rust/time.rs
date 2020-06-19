    #![allow(dead_code, unused_imports)]
    //! Rust wrappers for `Instant` / `Duration` classes
    use crate::dll::*;
    use std::ffi::c_void;


    /// `Instant` struct
    pub use crate::dll::AzInstantPtr as Instant;

    impl Instant {
        /// Creates a new `Instant` instance.
        pub fn now() -> Self { (crate::dll::get_azul_dll().az_instant_ptr_now)() }
    }

    impl std::fmt::Debug for Instant { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_instant_ptr_fmt_debug)(self)) } }
    impl Drop for Instant { fn drop(&mut self) { (crate::dll::get_azul_dll().az_instant_ptr_delete)(self); } }


    /// `Duration` struct
    pub use crate::dll::AzDuration as Duration;

    impl std::fmt::Debug for Duration { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_duration_fmt_debug)(self)) } }
    impl Clone for Duration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_duration_deep_copy)(self) } }
    impl Drop for Duration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_duration_delete)(self); } }
