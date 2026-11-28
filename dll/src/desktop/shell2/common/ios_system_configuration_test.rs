use super::*;

/// The violations of `scripts/ci/ios_symbol_gate.py` for AzMeet and AzCompanion (QA run 46089359452, `AzMeet.ios-sim.symbols.json`),
/// every one "not available on iOS at all", with the leading underscore of the Mach-O spelling.
const GATE_LISTED: [&str; 23] = [
    "_SCDynamicStoreCopyValue",
    "_SCDynamicStoreCreate",
    "_SCDynamicStoreCreateWithOptions",
    "_SCDynamicStoreKeyCreateNetworkGlobalEntity",
    "_SCDynamicStoreKeyCreateNetworkServiceEntity",
    "_SCNetworkInterfaceCopyAll",
    "_SCNetworkInterfaceGetBSDName",
    "_SCNetworkInterfaceGetInterfaceType",
    "_SCNetworkInterfaceGetLocalizedDisplayName",
    "_SCNetworkProtocolGetConfiguration",
    "_SCNetworkServiceCopyAll",
    "_SCNetworkServiceCopyProtocol",
    "_SCNetworkServiceGetInterface",
    "_SCNetworkServiceGetName",
    "_SCPreferencesCreate",
    "_kSCDynamicStoreDomainState",
    "_kSCDynamicStorePropNetPrimaryService",
    "_kSCDynamicStoreUseSessionKeys",
    "_kSCEntNetDNS",
    "_kSCEntNetIPv4",
    "_kSCNetworkProtocolTypeIPv4",
    "_kSCNetworkProtocolTypeIPv6",
    "_kSCPropNetDNSServerAddresses",
];

#[test]
fn the_table_holds_exactly_the_symbols_the_symbol_gate_listed() {
    for listed in GATE_LISTED {
        assert!(is_shimmed(listed), "{listed} is listed by the gate and has no entry");
    }
    assert_eq!(SHIMMED.len(), GATE_LISTED.len(), "an entry the gate never listed (or a duplicate)");
    for symbol in &SHIMMED {
        assert!(
            GATE_LISTED.iter().any(|listed| listed.trim_start_matches('_') == symbol.name),
            "{} is in the table and was never listed by the gate",
            symbol.name
        );
    }
}

#[test]
fn the_table_is_sorted_and_has_no_name_twice() {
    for pair in SHIMMED.windows(2) {
        assert!(pair[0].name < pair[1].name, "{} before {}", pair[0].name, pair[1].name);
    }
}

#[test]
fn a_name_is_found_with_or_without_the_underscore_of_the_object_file() {
    assert_eq!(shim_of("SCDynamicStoreCopyValue"), shim_of("_SCDynamicStoreCopyValue"));
    assert!(shim_of("SCDynamicStoreCopyValue").is_some());
    // SCDynamicStoreCopyProxies is available on iOS: the gate never listed it and the shim must never take it over
    assert!(!is_shimmed("SCDynamicStoreCopyProxies"));
    assert!(!is_shimmed("SCNetworkReachabilityCreateWithName"));
    assert!(!is_shimmed(""));
}

#[test]
fn constants_are_the_names_that_start_with_k_and_functions_the_ones_that_do_not() {
    for symbol in &SHIMMED {
        assert_eq!(
            symbol.kind == ShimKind::Constant,
            symbol.name.starts_with("kSC"),
            "{} is {:?}",
            symbol.name,
            symbol.kind
        );
    }
    assert_eq!(names_of(ShimKind::Constant).len(), 8);
    assert_eq!(SHIMMED.len() - names_of(ShimKind::Constant).len(), 15);
}

/// The Rust bindings of the dependencies (`objc2-system-configuration` 0.3) `expect` an object from these three and panic on NULL
/// ("function was marked as returning non-null, but actually returned NULL"): the interface list and the two key builders.
/// Every other function is declared `Option<..>` there and answers NULL.
#[test]
fn the_functions_whose_bindings_expect_an_object_answer_with_one() {
    assert_eq!(names_of(ShimKind::EmptyArray), ["SCNetworkInterfaceCopyAll"]);
    assert_eq!(
        names_of(ShimKind::EmptyString),
        ["SCDynamicStoreKeyCreateNetworkGlobalEntity", "SCDynamicStoreKeyCreateNetworkServiceEntity"]
    );
}

/// What stays when the lock file moves to netdev 0.47 (iroh 1.4 and later): the three symbols of the DNS resolver. The rest of the
/// table (and of the shim) can then be removed.
#[test]
fn three_symbols_belong_to_the_dns_resolver_and_stay_when_netdev_drops_ios() {
    let dns: Vec<&str> = SHIMMED.iter().filter(|s| s.user == DNS).map(|s| s.name).collect();
    assert_eq!(dns, ["SCDynamicStoreCopyValue", "SCDynamicStoreCreateWithOptions", "kSCDynamicStoreUseSessionKeys"]);
    assert!(SHIMMED.iter().all(|s| s.user == DNS || s.user == NETDEV));
}
