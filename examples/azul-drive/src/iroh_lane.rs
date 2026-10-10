//! AzDrive's iroh dialer: an Azlin drive's requests go over iroh to the nodes its last refresh
//! named with an iroh id first (azcloud-kit's lane: every ready node at its iroh sockets, probed,
//! a node that fails rests five minutes), through azul's iroh endpoint
//! ([`azul_storage::azul_iroh`]: S3 over iroh, no DNS); HTTPS with the failover's layers (the
//! block endpoint, the nodes, the failover URLs, the nodes' addresses) is the fallback. A plain
//! S3 bucket (AWS, R2, MinIO) never comes here: it opens through azul-storage, HTTPS only.

use std::sync::Arc;

use azcloud_kit::{transport::TransportPref, IrohDialer, IrohTarget};

/// The variable that names the transport (`auto`, `iroh` or `https`), as for `azcloud`.
pub(crate) const TRANSPORT_VAR: &str = "AZCLOUD_TRANSPORT";

/// The dialer every Azlin drive of the app goes through; `None` when `AZCLOUD_TRANSPORT=https`
/// (`env` reads a variable) asks for HTTPS only. `iroh` is taken as `auto`: a window keeps
/// working over HTTPS when no node answers over iroh.
pub(crate) fn dialer(env: &dyn Fn(&str) -> Option<String>) -> Option<Arc<dyn IrohDialer>> {
    let pref = env(TRANSPORT_VAR)
        .as_deref()
        .and_then(TransportPref::parse)
        .unwrap_or(TransportPref::Auto);
    if pref == TransportPref::Https {
        return None;
    }
    let dial = |target: &IrohTarget, relay: Option<&str>| {
        azul_storage::azul_iroh::dial(&target.id, &target.addrs, relay)
    };
    Some(Arc::new(dial))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An environment where only `AZCLOUD_TRANSPORT` is set, to `value`.
    fn env(value: &'static str) -> impl Fn(&str) -> Option<String> {
        move |name: &str| (name == TRANSPORT_VAR).then(|| value.to_string())
    }

    #[test]
    fn https_asked_for_means_no_iroh_dialer_and_anything_else_means_one() {
        assert!(dialer(&env("https")).is_none());
        assert!(dialer(&env(" HTTP ")).is_none());
        assert!(dialer(&env("auto")).is_some());
        assert!(dialer(&env("iroh")).is_some());
        assert!(dialer(&|_: &str| None).is_some(), "iroh first by default");
    }
}
