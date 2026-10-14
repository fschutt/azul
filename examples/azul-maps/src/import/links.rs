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
    let _ = url;
    None
}

/// The place's name in a Google Maps URL (`/maps/place/Brandenburger+Tor/...`).
#[must_use]
pub fn google_place_name(url: &str) -> Option<String> {
    let _ = url;
    None
}

/// The place a shared Apple Maps link names (`maps.apple.com/?ll=...&q=...`,
/// `maps.apple.com/place?coordinate=...&name=...`, `address=`).
#[must_use]
pub fn apple_maps_link(url: &str) -> Option<SharedPlace> {
    let _ = url;
    None
}

/// The place a pasted link names: Apple Maps' or Google Maps'.
#[must_use]
pub fn shared_link(url: &str) -> Option<SharedPlace> {
    let _ = url;
    None
}

/// `%xx` and `+` decoded.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    text.to_string()
}
