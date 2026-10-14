//! Links to places: the coordinates and the name in a Google Maps URL, and a shared Apple Maps
//! link (Apple Maps has no bulk export; a link shared from it is pasted one at a time).

/// A place a shared link names.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SharedPlace {
    pub name: String,
    /// Where it is, when the link says.
    pub at: Option<(f64, f64)>,
    pub address: String,
}

/// The coordinates in a Google Maps URL: the place's own (`!3d<lat>!4d<lon>`), else a
/// searched or shown place (`/search/<lat>,<lon>`, `/place/<lat>,<lon>`, `q=` / `ll=`), else
/// the view's centre (`@<lat>,<lon>,<zoom>z`).
#[must_use]
pub fn google_coordinates(url: &str) -> Option<(f64, f64)> {
    let url = percent_decode(url);
    // The place's own position: `!3d<lat>!4d<lon>` in the data part.
    if let (Some(lat), Some(lon)) = (number_after(&url, "!3d"), number_after(&url, "!4d")) {
        if let Some(at) = on_earth(lat, lon) {
            return Some(at);
        }
    }
    for marker in ["/search/", "/place/", "?q=", "&q=", "?ll=", "&ll=", "?query=", "&query="] {
        if let Some(at) = url.find(marker).and_then(|i| pair(&url[i + marker.len()..])) {
            return Some(at);
        }
    }
    // The view's centre: `@<lat>,<lon>,<zoom>z`.
    url.find('@').and_then(|i| pair(&url[i + 1..]))
}

/// The number right after `marker` in `text`.
fn number_after(text: &str, marker: &str) -> Option<f64> {
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// `<lat>,<lon>` at the start of `text` (spaces after the comma allowed), when it is a place
/// on Earth.
fn pair(text: &str) -> Option<(f64, f64)> {
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == ',' || c == ' '))
        .unwrap_or(text.len());
    let mut numbers = text[..end].split(',').map(str::trim);
    let lat: f64 = numbers.next()?.parse().ok()?;
    let lon: f64 = numbers.next()?.parse().ok()?;
    on_earth(lat, lon)
}

fn on_earth(lat: f64, lon: f64) -> Option<(f64, f64)> {
    (lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

/// The place's name in a Google Maps URL (`/maps/place/Brandenburger+Tor/...`).
#[must_use]
pub fn google_place_name(url: &str) -> Option<String> {
    let start = url.find("/maps/place/")? + "/maps/place/".len();
    let rest = &url[start..];
    let end = rest.find(['/', '?', '@']).unwrap_or(rest.len());
    let name = percent_decode(&rest[..end]).trim().to_string();
    (!name.is_empty() && pair(&name).is_none()).then_some(name)
}

/// The query parameters of a URL, decoded.
fn query_params(url: &str) -> Vec<(String, String)> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    let query = query.split('#').next().unwrap_or("");
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

/// The place a shared Apple Maps link names (`maps.apple.com/?ll=...&q=...`,
/// `maps.apple.com/place?coordinate=...&name=...`, `address=`).
#[must_use]
pub fn apple_maps_link(url: &str) -> Option<SharedPlace> {
    let url = url.trim();
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?
        .split(['/', '?'])
        .next()?;
    if !host.eq_ignore_ascii_case("maps.apple.com") {
        return None;
    }
    let params = query_params(url);
    let get = |key: &str| params.iter().find(|(k, _)| k == key).map(|(_, v)| v.trim().to_string());
    let at = ["coordinate", "ll", "sll"].iter().find_map(|k| get(k).and_then(|v| pair(&v)));
    let name = get("name").or_else(|| get("q")).unwrap_or_default();
    let address = get("address").unwrap_or_default();
    if name.is_empty() && address.is_empty() && at.is_none() {
        return None;
    }
    Some(SharedPlace { name, at, address })
}

/// The place a pasted link names: Apple Maps' or Google Maps'.
#[must_use]
pub fn shared_link(url: &str) -> Option<SharedPlace> {
    let url = url.trim();
    if let Some(place) = apple_maps_link(url) {
        return Some(place);
    }
    let lower = url.to_ascii_lowercase();
    let google = (lower.starts_with("https://") || lower.starts_with("http://"))
        && (lower.contains("google.") && lower.contains("/maps") || lower.contains("maps.google.") || lower.contains("goo.gl/maps"));
    if !google {
        return None;
    }
    let at = google_coordinates(url);
    let name = google_place_name(url).unwrap_or_default();
    if at.is_none() && name.is_empty() {
        return None;
    }
    Some(SharedPlace {
        name,
        at,
        address: String::new(),
    })
}

/// `%xx` and `+` decoded.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
