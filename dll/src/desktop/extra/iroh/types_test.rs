use azul_css::{AzString, StringVec};

use super::*;

#[test]
fn a_new_config_has_local_discovery_on_and_the_setter_switches_it() {
    let config = IrohConfig::create(AzString::from_const_str("app/1"));
    assert!(config.local_discovery, "local discovery is on by default");

    let off = config.clone().with_local_discovery(false);
    assert!(!off.local_discovery);
    assert!(off.clone().with_local_discovery(true).local_discovery);

    // the setter changes that field and nothing else
    assert_eq!(
        IrohConfig {
            local_discovery: true,
            ..off
        },
        config
    );
}

#[test]
fn a_new_config_advertises_nothing_and_the_setter_sets_that_field_only() {
    let config = IrohConfig::create(AzString::from_const_str("app/1"));
    assert_eq!(
        config.advertised.as_str(),
        "",
        "nothing is advertised by default"
    );

    let named = config
        .clone()
        .with_advertised(AzString::from_const_str("azreview/1;n=Studio Mac"));
    assert_eq!(named.advertised.as_str(), "azreview/1;n=Studio Mac");
    assert_eq!(
        IrohConfig {
            advertised: AzString::from_const_str(""),
            ..named
        },
        config
    );
}

#[test]
fn the_nearby_events_and_the_nearby_peer_are_what_the_c_api_names() {
    // appended to the enum, so the values of the kinds that existed do not move
    assert_eq!(IrohEventKind::Error as u8, 5);
    assert_eq!(IrohEventKind::NearbyAppeared as u8, 6);
    assert_eq!(IrohEventKind::NearbyGone as u8, 7);
    let peer = IrohNearbyPeer {
        endpoint_id: AzString::from_const_str("abc"),
        advertised: AzString::from_const_str(""),
        addresses: StringVec::from_vec(Vec::new()),
    };
    assert_eq!(
        IrohNearbyPeerVec::from_vec(vec![peer.clone()]).as_ref(),
        &[peer]
    );
}
