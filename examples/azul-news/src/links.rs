//! Web addresses: a relative link made absolute against the address it was found at, the
//! tracking parameters taken off a link (the "strip tracking parameters" setting), the site's
//! name for the list. On the `url` crate (WHATWG URL parsing, already in Cargo.lock).

/// `href` as an absolute address: as it is when it is one, else joined onto `base` (a feed's,
/// an article's or an `xml:base` address); trimmed; `""` for an empty `href`; `href` as it is
/// when neither makes an address.
#[must_use]
pub fn resolve(base: &str, href: &str) -> String {
    let href = href.trim();
    if href.is_empty() {
        return String::new();
    }
    if let Ok(absolute) = url::Url::parse(href) {
        return absolute.to_string();
    }
    match url::Url::parse(base.trim()).and_then(|b| b.join(href)) {
        Ok(joined) => joined.to_string(),
        Err(_) => href.to_string(),
    }
}

/// The query parameters that only tell a site where a click came from.
const TRACKING: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "msclkid",
    "yclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "oly_anon_id",
    "oly_enc_id",
    "vero_id",
    "wickedid",
    "__s",
    "rb_clickid",
    "s_cid",
    "ncid",
    "sr_share",
];

/// Whether a query parameter's name is a tracking one (`utm_*` and the [`TRACKING`] list).
#[must_use]
pub fn is_tracking(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("utm_") || TRACKING.contains(&lower.as_str())
}

/// `link` without its tracking parameters (the rest of the query as it was written); as it is
/// when it has none or is no address.
#[must_use]
pub fn strip_tracking(link: &str) -> String {
    let Ok(mut url) = url::Url::parse(link.trim()) else {
        return link.to_string();
    };
    let Some(query) = url.query().map(str::to_string) else {
        return link.to_string();
    };
    let parts: Vec<&str> = query.split('&').filter(|p| !p.is_empty()).collect();
    let kept: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|p| !is_tracking(p.split('=').next().unwrap_or("")))
        .collect();
    if kept.len() == parts.len() {
        return link.to_string();
    }
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.set_query(Some(kept.join("&").as_str()));
    }
    url.to_string()
}

/// The site's name for the list: the host without `www.` (`""` when there is none).
#[must_use]
pub fn site_name(link: &str) -> String {
    url::Url::parse(link.trim())
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_default()
}

/// Whether the address is on a host reserved for documentation (RFC 2606 / RFC 6761:
/// `example.com`, `example.net`, `example.org` and their subdomains, `*.example`,
/// `*.invalid`): no feed is ever there - the `--sample` library's addresses, which a refresh
/// does not ask.
#[must_use]
pub fn is_documentation_host(link: &str) -> bool {
    let Some(host) = url::Url::parse(link.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    let host = host.trim_end_matches('.');
    ["example.com", "example.net", "example.org"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        || host.ends_with(".example")
        || host.ends_with(".invalid")
}

/// Whether the address is on the web (`http:` / `https:`) - what may be fetched.
#[must_use]
pub fn is_web(link: &str) -> bool {
    let lower = link.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_link_is_made_absolute_against_its_base() {
        let base = "https://example.org/blog/2026/post.html";
        assert_eq!(
            resolve(base, "images/a.png"),
            "https://example.org/blog/2026/images/a.png"
        );
        assert_eq!(resolve(base, "/feed/"), "https://example.org/feed/");
        assert_eq!(
            resolve(base, "//cdn.example.net/x.jpg"),
            "https://cdn.example.net/x.jpg"
        );
        assert_eq!(
            resolve(base, "../up.html"),
            "https://example.org/blog/up.html"
        );
        assert_eq!(
            resolve(base, "  https://other.example.net/  "),
            "https://other.example.net/"
        );
        assert_eq!(
            resolve(base, "mailto:ida@example.org"),
            "mailto:ida@example.org"
        );
        assert_eq!(resolve(base, ""), "");
        assert_eq!(
            resolve("", "relative.html"),
            "relative.html",
            "no base: as it is"
        );
        assert_eq!(resolve("not a url", "a.html"), "a.html");
    }

    #[test]
    fn tracking_parameters_are_taken_off_and_the_rest_stays() {
        assert_eq!(
            strip_tracking(
                "https://example.org/a?id=7&utm_source=rss&utm_medium=feed&fbclid=xyz#top"
            ),
            "https://example.org/a?id=7#top"
        );
        assert_eq!(
            strip_tracking("https://example.org/a?utm_campaign=x"),
            "https://example.org/a"
        );
        assert_eq!(
            strip_tracking("https://example.org/search?q=a+b&page=2"),
            "https://example.org/search?q=a+b&page=2",
            "a link without tracking is left exactly as it was"
        );
        assert_eq!(strip_tracking("no link"), "no link");
        assert!(is_tracking("UTM_Source"));
        assert!(!is_tracking("id"));
    }

    #[test]
    fn documentation_hosts_are_known_and_real_ones_are_not() {
        assert!(is_documentation_host("https://weekly.example.org/feed.xml"));
        assert!(is_documentation_host("https://example.com/"));
        assert!(is_documentation_host("http://news.example.net/rss"));
        assert!(is_documentation_host("https://feeds.example/rss"));
        assert!(is_documentation_host("https://nothing.invalid/"));
        assert!(!is_documentation_host(
            "https://blog.rust-lang.org/feed.xml"
        ));
        assert!(!is_documentation_host("https://notexample.org/feed"));
        assert!(!is_documentation_host("http://127.0.0.1:8790/feed.xml"));
        assert!(!is_documentation_host("not a link"));
    }

    #[test]
    fn the_site_name_is_the_host_without_www() {
        assert_eq!(site_name("https://www.example.org/feed/"), "example.org");
        assert_eq!(
            site_name("http://news.example.net:8080/x"),
            "news.example.net"
        );
        assert_eq!(site_name("not a link"), "");
        assert!(is_web("HTTPS://example.org"));
        assert!(!is_web("file:///etc/passwd"));
        assert!(!is_web("javascript:alert(1)"));
    }
}
