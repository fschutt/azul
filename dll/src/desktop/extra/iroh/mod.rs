//! `azul.iroh`: peer-to-peer QUIC connections with hole punching and relay fallback.
//!
//! The `IrohEndpoint` handle is always compiled; the iroh engine behind it needs the `iroh`
//! feature on a target its crypto provider builds for (`cfg(az_iroh_engine)`, see build.rs).
//! Delivery is poll-based: drain `recv` from a timer.

pub mod loadbalancer;
mod types;

#[cfg(az_iroh_engine)]
mod engine;

use core::ffi::c_void;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use azul_css::{AzString, U8Vec};

pub use self::types::*;

/// A QUIC endpoint that dials and accepts peers by ticket and exchanges frames and messages with them.
///
/// Copies share one endpoint; it closes when the last copy is dropped or `close` is called.
#[repr(C)]
#[derive(Debug)]
pub struct IrohEndpoint {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

struct Inner {
    #[cfg(az_iroh_engine)]
    engine: Option<engine::Engine>,
    errors: Mutex<VecDeque<IrohEvent>>,
}

impl Clone for IrohEndpoint {
    fn clone(&self) -> Self {
        if self.ptr.is_null() {
            return Self::default();
        }
        unsafe { Arc::increment_strong_count(self.ptr.cast_const().cast::<Inner>()) };
        IrohEndpoint {
            ptr: self.ptr,
            run_destructor: true,
        }
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
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            drop(unsafe { Arc::from_raw(self.ptr.cast_const().cast::<Inner>()) });
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl IrohEndpoint {
    fn from_inner(inner: Inner) -> Self {
        IrohEndpoint {
            ptr: Arc::into_raw(Arc::new(inner)).cast_mut().cast::<c_void>(),
            run_destructor: true,
        }
    }

    fn failed(reason: String) -> Self {
        Self::from_inner(Inner {
            #[cfg(az_iroh_engine)]
            engine: None,
            errors: Mutex::new(VecDeque::from([IrohEvent::new(
                IrohEventKind::Error,
                0,
                reason,
            )])),
        })
    }

    fn inner(&self) -> Option<&Inner> {
        unsafe { self.ptr.cast_const().cast::<Inner>().as_ref() }
    }

    #[cfg(az_iroh_engine)]
    fn engine(&self) -> Option<&engine::Engine> {
        self.inner()?.engine.as_ref()
    }

    fn report(&self, reason: String) {
        if let Some(inner) = self.inner() {
            let mut errors = inner.errors.lock().unwrap_or_else(|e| e.into_inner());
            errors.push_back(IrohEvent::new(IrohEventKind::Error, 0, reason));
        }
    }

    /// Binds an endpoint. On failure `is_bound` is false and the first `recv` returns the `Error` event.
    pub fn bind(config: IrohConfig) -> Self {
        #[cfg(az_iroh_engine)]
        {
            match engine::Engine::bind(&config) {
                Ok(engine) => Self::from_inner(Inner {
                    engine: Some(engine),
                    errors: Mutex::default(),
                }),
                Err(reason) => Self::failed(reason),
            }
        }
        #[cfg(not(az_iroh_engine))]
        {
            let _ = config;
            let reason = "this build has no iroh engine: rebuild azul-dll with --features iroh on a \
             target the transport supports";
            static ANNOUNCE: std::sync::Once = std::sync::Once::new();
            ANNOUNCE.call_once(|| eprintln!("[azul][iroh] IrohEndpoint::bind: {reason}"));
            Self::failed(reason.to_string())
        }
    }

    /// Whether the endpoint is bound and not closed.
    pub fn is_bound(&self) -> bool {
        #[cfg(az_iroh_engine)]
        {
            self.engine().is_some_and(|engine| engine.is_bound())
        }
        #[cfg(not(az_iroh_engine))]
        {
            false
        }
    }

    /// Public key identifying this endpoint, as lowercase hex. Empty when not bound.
    pub fn endpoint_id(&self) -> AzString {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return AzString::from_string(engine.endpoint_id());
        }
        AzString::from_const_str("")
    }

    /// The 32-byte identity key, for `IrohConfig::with_secret_key` on the next start. Empty when not bound.
    pub fn secret_key(&self) -> U8Vec {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return U8Vec::from_vec(engine.secret_key());
        }
        U8Vec::from_const_slice(&[])
    }

    /// Dialing string with this endpoint's id and current addresses. Empty when not bound.
    pub fn ticket(&self) -> AzString {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return AzString::from_string(engine.ticket());
        }
        AzString::from_const_str("")
    }

    /// Dials the endpoint in `ticket`, or a bare endpoint id. The outcome arrives as `PeerConnected` or `Error`.
    ///
    /// Returns false, and queues the `Error` event, when the ticket cannot be dialed at all.
    pub fn connect(&self, ticket: AzString) -> bool {
        #[cfg(az_iroh_engine)]
        let result = match self.engine() {
            Some(engine) => engine.connect(ticket.as_str()),
            None => Err("the endpoint is not bound".to_string()),
        };
        #[cfg(not(az_iroh_engine))]
        let result: Result<(), String> = {
            let _ = ticket;
            Err("this build has no iroh engine".to_string())
        };
        match result {
            Ok(()) => true,
            Err(reason) => {
                self.report(reason);
                false
            }
        }
    }

    /// Queues a frame for one peer. A newer frame of the same track replaces one that has not left yet.
    pub fn send_frame(&self, peer: u64, track: u32, data: U8Vec) -> bool {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return engine.send_frame(Some(peer), track, data.as_ref());
        }
        let _ = (peer, track);
        drop(data);
        false
    }

    /// Queues a frame for every connected peer, replacing unsent frames of the track. False without peers.
    pub fn broadcast_frame(&self, track: u32, data: U8Vec) -> bool {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return engine.send_frame(None, track, data.as_ref());
        }
        let _ = track;
        drop(data);
        false
    }

    /// Sends a reliable message. The peer receives messages in the order they were sent.
    pub fn send_message(&self, peer: u64, data: U8Vec) -> bool {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return engine.send_message(peer, data.as_ref());
        }
        let _ = peer;
        drop(data);
        false
    }

    /// Closes the connection to `peer`. Returns false when no such connection is open.
    pub fn disconnect(&self, peer: u64) -> bool {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return engine.disconnect(peer);
        }
        let _ = peer;
        false
    }

    /// Number of open connections.
    pub fn peer_count(&self) -> usize {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return engine.peer_count();
        }
        0
    }

    /// Endpoint id of the peer behind a connection handle. Empty when the connection is gone.
    pub fn peer_endpoint_id(&self, peer: u64) -> AzString {
        #[cfg(az_iroh_engine)]
        if let Some(id) = self
            .engine()
            .and_then(|engine| engine.peer_endpoint_id(peer))
        {
            return AzString::from_string(id);
        }
        let _ = peer;
        AzString::from_const_str("")
    }

    /// Path and throughput statistics of a connection. All zero when the connection is gone.
    pub fn peer_stats(&self, peer: u64) -> IrohPeerStats {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            return engine.peer_stats(peer);
        }
        let _ = peer;
        IrohPeerStats::default()
    }

    /// Takes the next event. Connection events and messages come first, then the newest frame of each track in turn.
    pub fn recv(&self) -> OptionIrohEvent {
        let Some(inner) = self.inner() else {
            return OptionIrohEvent::None;
        };
        if let Some(error) = inner
            .errors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
        {
            return OptionIrohEvent::Some(error);
        }
        #[cfg(az_iroh_engine)]
        if let Some(event) = inner.engine.as_ref().and_then(|engine| engine.recv()) {
            return OptionIrohEvent::Some(event);
        }
        OptionIrohEvent::None
    }

    /// Closes the endpoint and its connections for every copy of this handle, then releases this copy.
    pub fn close(&mut self) {
        #[cfg(az_iroh_engine)]
        if let Some(engine) = self.engine() {
            engine.close();
        }
        if self.run_destructor && !self.ptr.is_null() {
            drop(unsafe { Arc::from_raw(self.ptr.cast_const().cast::<Inner>()) });
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

// netdev and n0-dns-resolver call SCNetworkInterface*/SCDynamicStore* without declaring the framework.
#[cfg(all(az_iroh_engine, target_os = "macos"))]
#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {}
