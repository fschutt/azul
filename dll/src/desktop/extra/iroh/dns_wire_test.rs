use std::str::FromStr;

use iroh::{EndpointId, SecretKey};

use super::*;

/// A name as it goes on the wire.
fn wire_name(labels: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for label in labels {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

fn srv_rdata(priority: u16, weight: u16, port: u16, name: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&priority.to_be_bytes());
    out.extend_from_slice(&weight.to_be_bytes());
    out.extend_from_slice(&port.to_be_bytes());
    out.extend_from_slice(name);
    out
}

#[test]
fn the_instance_label_is_the_base32_of_the_key_that_iroh_parses_back() {
    // the vectors of RFC 4648 section 10, lower case, without padding
    for (plain, base32) in [
        ("", ""),
        ("f", "my"),
        ("fo", "mzxq"),
        ("foo", "mzxw6"),
        ("foob", "mzxw6yq"),
        ("fooba", "mzxw6ytb"),
        ("foobar", "mzxw6ytboi"),
    ] {
        assert_eq!(base32_lower(plain.as_bytes()), base32, "{plain:?}");
    }
    for seed in [0u8, 1, 7, 200, 255] {
        let id = SecretKey::from_bytes(&[seed; 32]).public();
        let label = instance_label(id.as_bytes());
        assert_eq!(label.len(), 52, "{label}");
        assert!(label.bytes().all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)));
        assert_eq!(EndpointId::from_str(&label).expect("iroh reads the label"), id);
        assert_eq!(
            instance_fullname(&label),
            format!("{label}._irohv1._udp.local.")
        );
    }
}

#[test]
fn an_srv_record_gives_its_port_and_its_host() {
    let target = wire_name(&["abc2345-41234", "local"]);
    let srv = parse_srv(&srv_rdata(0, 5, 41234, &target)).expect("a plain SRV record");
    assert_eq!(
        srv,
        Srv {
            priority: 0,
            weight: 5,
            port: 41234,
            target: "abc2345-41234.local.".to_string()
        }
    );
}

#[test]
fn a_hostile_srv_record_is_nothing_and_never_a_panic() {
    let good = srv_rdata(0, 0, 80, &wire_name(&["host", "local"]));
    assert!(parse_srv(&good).is_some());

    // every prefix of a good record is short or does not end its name
    for cut in 0..good.len() {
        assert!(parse_srv(&good[..cut]).is_none(), "cut at {cut}");
    }
    // bytes after the name
    let mut longer = good.clone();
    longer.push(0);
    assert!(parse_srv(&longer).is_none());
    // a compression pointer (a loop to itself), a reserved label type, a label over 63 bytes
    assert!(parse_srv(&srv_rdata(0, 0, 80, &[0xc0, 0x06])).is_none());
    assert!(parse_srv(&srv_rdata(0, 0, 80, &[0x80, 1, 2, 0])).is_none());
    assert!(parse_srv(&srv_rdata(0, 0, 80, &[0x40, 1, 2, 0])).is_none());
    let mut long_label = vec![64u8];
    long_label.extend_from_slice(&[b'a'; 64]);
    long_label.push(0);
    assert!(parse_srv(&srv_rdata(0, 0, 80, &long_label)).is_none());
    // a name over 255 bytes in labels of 63
    let mut long_name = Vec::new();
    for _ in 0..5 {
        long_name.push(63u8);
        long_name.extend_from_slice(&[b'a'; 63]);
    }
    long_name.push(0);
    assert!(parse_srv(&srv_rdata(0, 0, 80, &long_name)).is_none());
    // the root name ("service not available"), characters that are not host-name ones
    assert!(parse_srv(&srv_rdata(0, 0, 80, &[0])).is_none());
    assert!(parse_srv(&srv_rdata(0, 0, 80, &wire_name(&["ho st", "local"]))).is_none());
    assert!(parse_srv(&srv_rdata(0, 0, 80, &wire_name(&["h\u{e4}st", "local"]))).is_none());
    // noise of every length
    for length in 0..300usize {
        let noise: Vec<u8> = (0..length).map(|n| (n * 37 + length) as u8).collect();
        let _ = parse_srv(&noise);
    }
}

#[test]
fn a_bsd_sockaddr_gives_an_ip_and_a_wrong_one_gives_none() {
    let mut v4 = vec![16u8, 2, 0x04, 0xd2, 192, 168, 1, 8];
    v4.extend_from_slice(&[0; 8]);
    assert_eq!(ip_from_sockaddr(&v4), Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8))));

    let ip6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
    let mut v6 = vec![28u8, 30, 0, 80, 0, 0, 0, 0];
    v6.extend_from_slice(&ip6.octets());
    v6.extend_from_slice(&[0; 4]);
    assert_eq!(ip_from_sockaddr(&v6), Some(IpAddr::V6(ip6)));

    // another family, a short one, an empty one
    assert_eq!(ip_from_sockaddr(&[16, 18, 0, 0, 1, 2, 3, 4]), None);
    assert_eq!(ip_from_sockaddr(&v4[..15]), None);
    assert_eq!(ip_from_sockaddr(&v6[..27]), None);
    assert_eq!(ip_from_sockaddr(&[16]), None);
    assert_eq!(ip_from_sockaddr(&[]), None);
}

#[test]
fn the_socket_addresses_of_a_peer_are_the_usable_ones_of_its_targets_once_each() {
    let mut ports = BTreeMap::new();
    ports.insert("a-4000.local.".to_string(), 4000u16);
    ports.insert("a-4001.local.".to_string(), 4001u16);
    ports.insert("a-9.local.".to_string(), 0u16); // a port of 0 is not a dial
    let mut hosts = BTreeMap::new();
    hosts.insert(
        "a-4000.local.".to_string(),
        vec![
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)),
        ],
    );
    hosts.insert(
        "a-4001.local.".to_string(),
        vec![IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 8))],
    );
    hosts.insert(
        "a-9.local.".to_string(),
        vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))],
    );
    hosts.insert("elsewhere.local.".to_string(), vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))]);
    assert_eq!(
        assemble(&ports, &hosts),
        vec![
            "192.168.1.8:4000".parse::<SocketAddr>().unwrap(),
            "[2001:db8::8]:4001".parse::<SocketAddr>().unwrap(),
        ]
    );

    // a flood of targets is cut at the limit
    let mut ports = BTreeMap::new();
    let mut hosts = BTreeMap::new();
    for n in 0..1000u16 {
        let target = format!("t{n:04}.local.");
        ports.insert(target.clone(), 1000 + n);
        hosts.insert(target, vec![IpAddr::V4(Ipv4Addr::new(10, 0, (n >> 8) as u8, n as u8))]);
    }
    assert_eq!(assemble(&ports, &hosts).len(), MAX_ADDRESSES_PER_PEER);
}

#[test]
fn the_errors_of_the_system_are_told_in_words() {
    let denied = describe_error(-65570);
    assert!(denied.contains("Local Network") && denied.contains("NSBonjourServices"), "{denied}");
    assert!(describe_error(-65563).contains("not running"));
    assert!(describe_error(-1).contains("-1"));
}

#[test]
fn a_host_is_filed_under_one_spelling_in_lower_case_with_its_root_dot() {
    assert_eq!(host_key("abc-41234.local."), "abc-41234.local.");
    assert_eq!(host_key("abc-41234.local"), "abc-41234.local.");
    assert_eq!(host_key("ABC-41234.Local."), "abc-41234.local.");
    assert_eq!(host_key("Abc-41234.LOCAL"), "abc-41234.local.");
    // the system's spelling finds the SRV record's host in the maps the assembling reads
    let mut ports = BTreeMap::new();
    let mut hosts: BTreeMap<String, Vec<IpAddr>> = BTreeMap::new();
    ports.insert(host_key("abc-41234.local."), 41234u16);
    hosts
        .entry(host_key("ABC-41234.Local"))
        .or_default()
        .push(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8)));
    assert_eq!(
        assemble(&ports, &hosts),
        vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 8)), 41234)]
    );
}
