//! The engine starts no I/O before the platform has what it needs: on Android a bind that comes
//! before the activity exists (the library's constructor, `main`) is an error the app can read, and
//! never an abort.

use std::sync::{atomic::AtomicU64, Arc, Mutex};

use azul_css::AzString;
use iroh::Endpoint;

use super::*;
use crate::desktop::extra::iroh::{
    discovery::{Held, LocalDiscovery},
    engine::Engine,
    nearby::Nearby,
    types::{IrohConfig, IrohRelayMode},
};

/// A platform whose Android context does not exist yet.
struct NotReady;

impl Platform for NotReady {
    fn android_context_ready(&self) -> bool {
        false
    }
}

/// A platform that has its context.
struct Ready;

impl Platform for Ready {
    fn android_context_ready(&self) -> bool {
        true
    }
}

/// Counts the starts of local discovery: the first thing the engine does after the check.
#[derive(Default)]
struct Recording {
    started: Mutex<u32>,
}

impl LocalDiscovery for Recording {
    fn start(
        &self,
        _endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
        _nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        *self.started.lock().unwrap() += 1;
        Ok(None)
    }
}

/// A config without relays, so nothing leaves this computer.
fn config() -> IrohConfig {
    IrohConfig::create(AzString::from_const_str("azul/iroh-platform-test/1"))
        .with_relay_mode(IrohRelayMode::Disabled)
}

#[test]
fn a_platform_without_its_context_refuses_and_says_what_to_do() {
    let reason = require_context(&NotReady).expect_err("a platform that is not ready refuses");
    assert_eq!(reason, CONTEXT_NOT_READY);
    assert!(reason.contains("Android context is not ready"), "{reason}");
    assert!(reason.contains("after the window exists"), "{reason}");
    assert_eq!(require_context(&Ready), Ok(()));
}

#[test]
fn a_bind_before_the_context_exists_is_an_error_and_nothing_runs() {
    let discovery = Recording::default();
    let refused = match Engine::bind_on(&config(), &discovery, &NotReady) {
        Ok(_) => panic!("a bind before the context exists must be refused"),
        Err(reason) => reason,
    };
    assert_eq!(refused, CONTEXT_NOT_READY);
    assert_eq!(*discovery.started.lock().unwrap(), 0, "no lookup was started");

    // the same call, once the context exists
    let engine = match Engine::bind_on(&config(), &discovery, &Ready) {
        Ok(engine) => engine,
        Err(reason) => panic!("a platform that is ready binds: {reason}"),
    };
    assert!(engine.is_bound());
    assert_eq!(*discovery.started.lock().unwrap(), 1);
}

/// A computer that is not an Android phone has no context to wait for.
#[cfg(not(target_os = "android"))]
#[test]
fn the_platform_of_a_host_is_always_ready() {
    assert!(OsPlatform.android_context_ready());
}
