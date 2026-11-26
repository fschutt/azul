//! The pure part of the Bonjour lookup (`bonjour.rs`, iOS): what the system's `dns_sd` hands to a
//! callback, parsed and assembled without a system call, so every target tests it. Nothing in here
//! trusts its input: it is what a neighbour on the local network announced, as the system relayed it.
//!
//! A peer that announced itself through `iroh-mdns-address-lookup` is, on the link,
//! `<id>._irohv1._udp.local.` with one SRV record per UDP port (target `<id>-<port>.local.`), address
//! records on those targets, and a PTR record under `_irohv1._udp.local.` (swarm-discovery 0.6.5 and
//! newer: the PTR record and a TTL are what Bonjour browsing needs). `<id>` is the endpoint id as
//! base32 (RFC 4648, lower case, no padding): 52 characters.

use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use super::discovery::{usable_peer_addr, MAX_ADDRESSES_PER_PEER};

/// The service type an endpoint is announced under (iroh-mdns-address-lookup's `irohv1`, over UDP).
/// An iOS app lists exactly this in `NSBonjourServices`.
pub(super) const SERVICE_TYPE: &str = "_irohv1._udp";

/// The domain of multicast DNS.
pub(super) const DOMAIN: &str = "local.";

/// A DNS name is at most 255 bytes on the wire, a label at most 63.
const MAX_NAME: usize = 255;
const MAX_LABEL: usize = 63;

/// The instance name an endpoint is announced under: its 32-byte id as base32 (RFC 4648, lower
/// case, no padding), 52 characters.
pub(super) fn instance_label(id: &[u8; 32]) -> String {
    base32_lower(id)
}

/// RFC 4648 base32, lower case, without padding.
fn base32_lower(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 0x1f) as usize]));
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize]));
    }
    out
}

/// `<label>._irohv1._udp.local.`: the full name of an announced instance.
pub(super) fn instance_fullname(label: &str) -> String {
    format!("{label}.{SERVICE_TYPE}.{DOMAIN}")
}

/// What an SRV record says: the host that serves, and the UDP port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Srv {
    pub(super) priority: u16,
    pub(super) weight: u16,
    pub(super) port: u16,
    /// The host name, with its trailing dot.
    pub(super) target: String,
}

/// An SRV record's data (RFC 2782): priority, weight, port (each 16 bits, big endian), then the
/// target name. `dns_sd` delivers names uncompressed; a compression pointer, a label over 63
/// bytes, a name over 255, a name that does not end, bytes after the name, or a name that is not
/// plain host-name characters is `None`, never a panic.
pub(super) fn parse_srv(rdata: &[u8]) -> Option<Srv> {
    if rdata.len() < 7 {
        return None;
    }
    let priority = u16::from_be_bytes([rdata[0], rdata[1]]);
    let weight = u16::from_be_bytes([rdata[2], rdata[3]]);
    let port = u16::from_be_bytes([rdata[4], rdata[5]]);
    let target = parse_name(&rdata[6..])?;
    Some(Srv {
        priority,
        weight,
        port,
        target,
    })
}

/// A DNS name in wire form, uncompressed, that ends exactly at the end of `bytes`.
fn parse_name(bytes: &[u8]) -> Option<String> {
    let mut name = String::new();
    let mut at = 0usize;
    loop {
        let length = usize::from(*bytes.get(at)?);
        at += 1;
        if length == 0 {
            break;
        }
        // 0b11xxxxxx is a compression pointer, 0b01xxxxxx and 0b10xxxxxx are reserved
        if length > MAX_LABEL {
            return None;
        }
        let label = bytes.get(at..at + length)?;
        at += length;
        if !label
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
        {
            return None;
        }
        for b in label {
            name.push(char::from(*b));
        }
        name.push('.');
        if name.len() > MAX_NAME {
            return None;
        }
    }
    if at != bytes.len() || name.is_empty() {
        return None;
    }
    Some(name)
}

/// The address in a BSD `sockaddr` (`sa_len`, `sa_family`, then the family's fields), as the Apple
/// systems pass to `DNSServiceGetAddrInfo`'s callback. `None` for another family or a short one.
pub(super) fn ip_from_sockaddr(sockaddr: &[u8]) -> Option<IpAddr> {
    const AF_INET: u8 = 2;
    const AF_INET6: u8 = 30;
    match (*sockaddr.get(1)?, sockaddr.len()) {
        // struct sockaddr_in { len, family, port: u16, addr: [u8; 4], zero: [u8; 8] }
        (AF_INET, 16..) => {
            let octets: [u8; 4] = sockaddr.get(4..8)?.try_into().ok()?;
            Some(IpAddr::V4(Ipv4Addr::from(octets)))
        }
        // struct sockaddr_in6 { len, family, port: u16, flowinfo: u32, addr: [u8; 16], scope: u32 }
        (AF_INET6, 28..) => {
            let octets: [u8; 16] = sockaddr.get(8..24)?.try_into().ok()?;
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

/// What a lookup tells the user in words when `dns_sd` answers with `code`.
pub(super) fn describe_error(code: i32) -> String {
    match code {
        // kDNSServiceErr_PolicyDenied
        -65570 => "Local Network access is denied for this app: allow it in Settings > Privacy & \
                   Security > Local Network (and the app's Info.plist needs \
                   NSLocalNetworkUsageDescription and NSBonjourServices with _irohv1._udp)"
            .to_string(),
        // kDNSServiceErr_ServiceNotRunning
        -65563 => "the system's Bonjour service (mDNSResponder) is not running".to_string(),
        other => format!("the system's Bonjour answered with error {other}"),
    }
}

/// The key a host name is filed under while a lookup collects the SRV targets and the addresses of
/// their hosts: lower case with one trailing dot. DNS names are case-insensitive, and the system may
/// hand a name back in another case or without its root dot than the SRV record carried it in; two
/// spellings of one host must find each other.
pub(super) fn host_key(name: &str) -> String {
    name.to_string()
}

/// The socket addresses of one peer: for every SRV target its port with the addresses found for
/// that target, in the order of the target names, each address once, only those a dial can use
/// ([`usable_peer_addr`]), at most [`MAX_ADDRESSES_PER_PEER`].
pub(super) fn assemble(
    ports: &BTreeMap<String, u16>,
    hosts: &BTreeMap<String, Vec<IpAddr>>,
) -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = Vec::new();
    for (target, port) in ports {
        for ip in hosts.get(target).into_iter().flatten() {
            let addr = SocketAddr::new(*ip, *port);
            if out.len() < MAX_ADDRESSES_PER_PEER && usable_peer_addr(&addr) && !out.contains(&addr)
            {
                out.push(addr);
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "dns_wire_test.rs"]
mod tests;
