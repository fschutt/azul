use azul_css::AzString;
#[cfg(az_iroh_engine)]
use azul_css::StringVec;

use super::*;

#[test]
fn a_handle_that_is_not_bound_has_no_discovery_status_and_nothing_to_clear() {
    let handle = IrohEndpoint::default();
    assert_eq!(handle.local_discovery_status().as_str(), "");
    assert!(!handle.clear_allowed_peers());
}

#[test]
fn a_handle_that_is_not_bound_advertises_nothing_and_has_no_neighbours() {
    let handle = IrohEndpoint::default();
    assert!(!handle.set_advertised(AzString::from_const_str("azreview/1;n=Studio Mac")));
    assert!(handle.nearby_peers().as_ref().is_empty());
}

#[cfg(az_iroh_engine)]
#[test]
fn a_bound_handle_takes_an_advertised_text_and_lists_nobody_without_discovery() {
    let handle = IrohEndpoint::bind(quiet_config());
    assert!(handle.is_bound());
    // switched off: the text is kept for when discovery is on, nothing is announced and nobody is heard
    assert!(handle.set_advertised(AzString::from_const_str("azreview/1;n=Studio Mac")));
    assert!(handle.set_advertised(AzString::from_const_str("")));
    assert!(handle.nearby_peers().as_ref().is_empty());
    assert_eq!(handle.local_discovery_status().as_str(), "off");
}

#[cfg(az_iroh_engine)]
fn quiet_config() -> IrohConfig {
    IrohConfig::create(AzString::from_const_str("azul/iroh-test/1"))
        .with_relay_mode(IrohRelayMode::Disabled)
        .with_local_discovery(false)
}

/// The next event of `kind` from `endpoint` within 20 s; any other `Error` event is returned
/// through `errors`.
#[cfg(az_iroh_engine)]
fn next_of(endpoint: &IrohEndpoint, kind: IrohEventKind, errors: &mut Vec<String>) -> IrohEvent {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        match endpoint.recv().into_option() {
            Some(event) if event.kind == kind => return event,
            Some(event) if event.kind == IrohEventKind::Error => {
                errors.push(event.text.as_str().to_string())
            }
            Some(_) => {}
            None => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "no {kind:?} within 20 s"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[cfg(az_iroh_engine)]
#[test]
fn a_bound_handle_reports_its_discovery_and_goes_back_to_accepting_every_peer() {
    let host = IrohEndpoint::bind(quiet_config());
    assert!(host.is_bound());
    assert_eq!(host.local_discovery_status().as_str(), "off");

    let mut errors = Vec::new();
    let ticket = next_of(&host, IrohEventKind::Ready, &mut errors).text;
    assert!(host.set_allowed_peers(StringVec::from_vec(Vec::new())));

    let stranger = IrohEndpoint::bind(quiet_config());
    assert!(stranger.connect(ticket.clone()));
    // refused: the host names the stranger in an Error event and never has a peer
    let mut refused = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while refused.is_empty() {
        if let Some(event) = host.recv().into_option() {
            if event.kind == IrohEventKind::Error {
                refused.push(event.text.as_str().to_string());
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the stranger was not refused"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(refused[0].contains("not an allowed peer"), "{}", refused[0]);
    assert_eq!(host.peer_count(), 0);

    // the pairing window: any peer that knows the protocol is accepted again
    assert!(host.clear_allowed_peers());
    assert!(stranger.connect(ticket));
    let connected = next_of(&host, IrohEventKind::PeerConnected, &mut errors);
    assert_eq!(connected.text.as_str(), stranger.endpoint_id().as_str());
}
