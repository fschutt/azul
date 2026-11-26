//! Unified `azul.iroh` handles. See [`crate::unified`].
//!
//! Native re-exports the real engine. wasm shares the plain data and the loadbalancer source and
//! stubs the endpoint until iroh runs over WebTransport in the browser.

#[cfg(az_desktop)]
pub use crate::desktop::extra::iroh::*;

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[path = "../desktop/extra/iroh/types.rs"]
mod types;

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[path = "../desktop/extra/iroh/loadbalancer.rs"]
pub mod loadbalancer;

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
pub use self::types::*;

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
pub use self::wasm_stub::IrohEndpoint;

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
mod wasm_stub {
    use core::ffi::c_void;

    use azul_css::{AzString, StringVec, U8Vec};
    use azul_layout::callbacks::ResultU8VecString;

    use super::{IrohConfig, IrohNearbyPeerVec, IrohPeerStats, OptionIrohEvent};

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
        pub fn request(
            &self,
            _endpoint_id: AzString,
            _addresses: StringVec,
            _relay_url: AzString,
            _data: U8Vec,
            _half_close: bool,
            _timeout_secs: u32,
        ) -> ResultU8VecString {
            ResultU8VecString::Err(AzString::from_const_str(
                "iroh is not available in the browser yet",
            ))
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
        pub fn set_track_priority(&self, _track: u32, _priority: i32) -> bool {
            false
        }
        pub fn set_message_priority(&self, _priority: i32) -> bool {
            false
        }
        pub fn disconnect(&self, _peer: u64) -> bool {
            false
        }
        /// Nothing is bound on wasm: nothing changes.
        pub fn set_allowed_peers(&self, _peers: StringVec) -> bool {
            false
        }
        /// Nothing is bound on wasm: nothing changes.
        pub fn clear_allowed_peers(&self) -> bool {
            false
        }
        /// Nothing is bound on wasm.
        pub fn local_discovery_status(&self) -> AzString {
            AzString::from_const_str("")
        }
        /// Nothing is bound on wasm: nothing changes.
        pub fn set_advertised(&self, _text: AzString) -> bool {
            false
        }
        /// Nothing is bound on wasm: no endpoint is nearby.
        pub fn nearby_peers(&self) -> IrohNearbyPeerVec {
            IrohNearbyPeerVec::from_vec(Vec::new())
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
