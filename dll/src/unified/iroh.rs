//! Unified `azul.iroh` handles. See [`crate::unified`].
//!
//! Native re-exports the real engine. wasm shares the plain data and the loadbalancer source and
//! stubs the endpoint until iroh runs over WebTransport in the browser.

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::iroh::*;

#[cfg(target_arch = "wasm32")]
#[path = "../desktop/extra/iroh/types.rs"]
mod types;

#[cfg(target_arch = "wasm32")]
#[path = "../desktop/extra/iroh/loadbalancer.rs"]
pub mod loadbalancer;

#[cfg(target_arch = "wasm32")]
pub use self::types::*;

#[cfg(target_arch = "wasm32")]
pub use self::wasm_stub::IrohEndpoint;

#[cfg(target_arch = "wasm32")]
mod wasm_stub {
    use core::ffi::c_void;

    use azul_css::{AzString, U8Vec};

    use super::{IrohConfig, IrohPeerStats, OptionIrohEvent};

    /// wasm stub of the desktop `IrohEndpoint`, with the same `#[repr(C)]` layout.
    #[repr(C)]
    #[derive(Debug)]
    pub struct IrohEndpoint {
        pub ptr: *mut c_void,
        pub run_destructor: bool,
    }

    impl Clone for IrohEndpoint {
        fn clone(&self) -> Self {
            IrohEndpoint::default()
        }
    }

    impl Default for IrohEndpoint {
        fn default() -> Self {
            IrohEndpoint {
                ptr: core::ptr::null_mut(),
                run_destructor: false,
            }
        }
    }

    impl Drop for IrohEndpoint {
        fn drop(&mut self) {}
    }

    impl IrohEndpoint {
        pub fn bind(_config: IrohConfig) -> IrohEndpoint {
            IrohEndpoint::default()
        }
        pub fn is_bound(&self) -> bool {
            false
        }
        pub fn endpoint_id(&self) -> AzString {
            AzString::from_const_str("")
        }
        pub fn secret_key(&self) -> U8Vec {
            U8Vec::from_const_slice(&[])
        }
        pub fn ticket(&self) -> AzString {
            AzString::from_const_str("")
        }
        pub fn connect(&self, _ticket: AzString) -> bool {
            false
        }
        pub fn send_frame(&self, _peer: u64, _track: u32, _data: U8Vec) -> bool {
            false
        }
        pub fn broadcast_frame(&self, _track: u32, _data: U8Vec) -> bool {
            false
        }
        pub fn send_message(&self, _peer: u64, _data: U8Vec) -> bool {
            false
        }
        pub fn disconnect(&self, _peer: u64) -> bool {
            false
        }
        pub fn peer_count(&self) -> usize {
            0
        }
        pub fn peer_endpoint_id(&self, _peer: u64) -> AzString {
            AzString::from_const_str("")
        }
        pub fn peer_stats(&self, _peer: u64) -> IrohPeerStats {
            IrohPeerStats::default()
        }
        pub fn recv(&self) -> OptionIrohEvent {
            OptionIrohEvent::None
        }
        pub fn close(&mut self) {}
    }
}
