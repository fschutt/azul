//! The SystemConfiguration symbols that iOS's SDK marks "unavailable" and that the networking dependencies of the `iroh`
//! feature still reference when they are built for iOS (engine ledger E63: AzMeet and AzCompanion did not build for iOS
//! because `scripts/ci/ios_symbol_gate.py` found 23 of them in the binary). Host-compiled and tested like `ios_logic`;
//! `shell2/ios/system_configuration.rs` defines every symbol of the table.
//!
//! WHO REFERENCES THEM. Two dependencies, and both only to read the network configuration of a Mac:
//!
//! * `n0-dns-resolver` (0.1.x, the DNS resolver of `iroh-dns`) reads the system's DNS servers through the `system-configuration`
//!   crate on EVERY Apple target: `SCDynamicStoreCreateWithOptions`, `SCDynamicStoreCopyValue`, the option key
//!   `kSCDynamicStoreUseSessionKeys`. When it cannot read them it logs a warning and uses public resolvers (its own fallback,
//!   `read_system` in its `system_config.rs`).
//! * `netdev` up to 0.46 (the interface lister of `netwatch` 0.19, iroh 1.2) has an iOS module that lists interfaces and DNS
//!   servers with the dynamic store and the network preferences: 20 more symbols. netdev 0.47 (netwatch 0.20, iroh 1.4 and later)
//!   drops them on iOS: once the lock file moves there, only the three of `n0-dns-resolver` remain and the rest of the table can go.
//!
//! WHAT iOS DOES. The dynamic store and the network preferences are macOS API (`API_UNAVAILABLE(ios)` in the headers): an app on
//! iOS has no way to read them, and Apple's review looks for exactly such symbols. So the answer that is true on iOS is "nothing is
//! configured here": no store, no value, no interfaces, no services. The dependencies handle that answer on their own error path.
//!
//! WHAT THE SHIM ANSWERS (`ShimKind`): NULL where the Rust bindings accept NULL; where they `expect` an object (the interface list
//! of `SCNetworkInterface::all`, the key builders of the dynamic store) an empty array or an empty string; the constants hold
//! `kCFNull`, the one immortal CoreFoundation object that can be retained, released, hashed and compared without ever being read as
//! text (`system-configuration` wraps `kSCDynamicStoreUseSessionKeys` under the create rule and releases it each time it builds a
//! store: a string made at start-up would be freed by the second call, the immortal object cannot be).

/// What a shimmed symbol answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShimKind {
    /// A function that answers NULL: no store, no value, no name.
    Null,
    /// A function whose Rust binding `expect`s a non-null result: an empty `CFArray` (made with `CFArrayCreate`).
    EmptyArray,
    /// A function whose Rust binding `expect`s a non-null result: an empty `CFString`.
    EmptyString,
    /// A `const CFStringRef` the dependency reads: it holds `kCFNull` (set before `main`).
    Constant,
}

/// One symbol of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShimmedSymbol {
    /// The C name, without the leading underscore of the Mach-O symbol.
    pub name: &'static str,
    /// What it answers.
    pub kind: ShimKind,
    /// The crate that references it on iOS.
    pub user: &'static str,
}

const DNS: &str = "n0-dns-resolver (system-configuration)";
const NETDEV: &str = "netdev <= 0.46";

const fn symbol(name: &'static str, kind: ShimKind, user: &'static str) -> ShimmedSymbol {
    ShimmedSymbol { name, kind, user }
}

/// Every symbol the symbol gate listed for AzMeet and AzCompanion (QA run 46089359452), sorted by name.
pub static SHIMMED: [ShimmedSymbol; 23] = [
    symbol("SCDynamicStoreCopyValue", ShimKind::Null, DNS),
    symbol("SCDynamicStoreCreate", ShimKind::Null, NETDEV),
    symbol("SCDynamicStoreCreateWithOptions", ShimKind::Null, DNS),
    symbol("SCDynamicStoreKeyCreateNetworkGlobalEntity", ShimKind::EmptyString, NETDEV),
    symbol("SCDynamicStoreKeyCreateNetworkServiceEntity", ShimKind::EmptyString, NETDEV),
    symbol("SCNetworkInterfaceCopyAll", ShimKind::EmptyArray, NETDEV),
    symbol("SCNetworkInterfaceGetBSDName", ShimKind::Null, NETDEV),
    symbol("SCNetworkInterfaceGetInterfaceType", ShimKind::Null, NETDEV),
    symbol("SCNetworkInterfaceGetLocalizedDisplayName", ShimKind::Null, NETDEV),
    symbol("SCNetworkProtocolGetConfiguration", ShimKind::Null, NETDEV),
    symbol("SCNetworkServiceCopyAll", ShimKind::Null, NETDEV),
    symbol("SCNetworkServiceCopyProtocol", ShimKind::Null, NETDEV),
    symbol("SCNetworkServiceGetInterface", ShimKind::Null, NETDEV),
    symbol("SCNetworkServiceGetName", ShimKind::Null, NETDEV),
    symbol("SCPreferencesCreate", ShimKind::Null, NETDEV),
    symbol("kSCDynamicStoreDomainState", ShimKind::Constant, NETDEV),
    symbol("kSCDynamicStorePropNetPrimaryService", ShimKind::Constant, NETDEV),
    symbol("kSCDynamicStoreUseSessionKeys", ShimKind::Constant, DNS),
    symbol("kSCEntNetDNS", ShimKind::Constant, NETDEV),
    symbol("kSCEntNetIPv4", ShimKind::Constant, NETDEV),
    symbol("kSCNetworkProtocolTypeIPv4", ShimKind::Constant, NETDEV),
    symbol("kSCNetworkProtocolTypeIPv6", ShimKind::Constant, NETDEV),
    symbol("kSCPropNetDNSServerAddresses", ShimKind::Constant, NETDEV),
];

/// The entry of a symbol, by the name the gate prints (a leading underscore, the Mach-O spelling, is allowed).
#[must_use]
pub fn shim_of(name: &str) -> Option<&'static ShimmedSymbol> {
    let name = name.strip_prefix('_').unwrap_or(name);
    SHIMMED.iter().find(|s| s.name == name)
}

/// Whether the shim defines this symbol (so that the symbol gate does not see it as an import).
#[must_use]
pub fn is_shimmed(name: &str) -> bool {
    shim_of(name).is_some()
}

/// The names of the symbols of one kind, in table order.
#[must_use]
pub fn names_of(kind: ShimKind) -> Vec<&'static str> {
    SHIMMED.iter().filter(|s| s.kind == kind).map(|s| s.name).collect()
}

#[cfg(test)]
#[path = "ios_system_configuration_test.rs"]
mod ios_system_configuration_test;
