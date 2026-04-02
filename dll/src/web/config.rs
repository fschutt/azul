//! Parse `AZ_BACKEND=web://ip:port` URL format.

use std::net::SocketAddr;

/// Parse a `web://ip:port` URL string into a `SocketAddr`.
///
/// Accepted formats:
/// - `web://127.0.0.1:8080`
/// - `web://0.0.0.0:3000`
/// - `web://[::1]:8080` (IPv6)
///
/// Returns `None` if the string doesn't start with `web://` or the
/// address cannot be parsed.
pub fn parse_web_url(s: &str) -> Option<SocketAddr> {
    let s = s.trim();

    // Strip the web:// prefix (case-insensitive)
    let addr_str = if s.len() > 6 && s[..6].eq_ignore_ascii_case("web://") {
        &s[6..]
    } else {
        return None;
    };

    // Strip optional query string (?tls=cert.pem etc.)
    let addr_str = addr_str.split('?').next().unwrap_or(addr_str);

    // Try parsing as SocketAddr
    addr_str.parse::<SocketAddr>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

    #[test]
    fn parse_ipv4() {
        assert_eq!(
            parse_web_url("web://127.0.0.1:8080"),
            Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080)))
        );
    }

    #[test]
    fn parse_ipv4_all_interfaces() {
        assert_eq!(
            parse_web_url("web://0.0.0.0:3000"),
            Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 3000)))
        );
    }

    #[test]
    fn parse_ipv6() {
        assert_eq!(
            parse_web_url("web://[::1]:8080"),
            Some(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0)))
        );
    }

    #[test]
    fn parse_with_query_string() {
        assert_eq!(
            parse_web_url("web://0.0.0.0:443?tls=cert.pem"),
            Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 443)))
        );
    }

    #[test]
    fn parse_case_insensitive() {
        assert!(parse_web_url("WEB://127.0.0.1:8080").is_some());
        assert!(parse_web_url("Web://127.0.0.1:8080").is_some());
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_web_url("headless"), None);
        assert_eq!(parse_web_url("web://"), None);
        assert_eq!(parse_web_url("web://not-an-address"), None);
        assert_eq!(parse_web_url("http://127.0.0.1:8080"), None);
    }
}
