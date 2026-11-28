//! What AzMaps shows besides the map, as plain data (no azul types): which of the three views the
//! panel (a desktop) or the sheet (a phone) holds, the place it shows, the chips under the search
//! bar and how the window is cut.
//!
//! ```text
//!            search a name / tap a result            Route
//!   Browse ---------------------------------> Place ---------> Directions
//!     ^ search field, chips, results, recents   ^  name, Route / Save / Share  |  from, to, modes, steps
//!     |                                         |                              |
//!     +------------- back / clear --------------+----------- back ------------+
//! ```
//!
//! A window at least [`WIDE_WIDTH`] wide has the panel at the left of the map; a narrower one (a
//! phone upright) the search bar with its chips on top of the map and the sheet from the bottom.

/// A window at least this wide (a desktop, a tablet, a phone turned on its side) has the panel at
/// the left; a narrower one has the search bar with its chips at the top and the sheet at the
/// bottom.
pub const WIDE_WIDTH: f32 = 560.0;

/// Whether a window `width` px wide gets the panel at the left (a width that is not a number: the
/// narrow layout, which fits every screen).
#[must_use]
pub fn is_wide(width: f32) -> bool {
    width.is_finite() && width >= WIDE_WIDTH
}

/// Which of the three views the panel (a desktop) or the sheet (a phone) holds.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum Panel {
    /// The search field, the chips, then the results of what was typed, or the recent and the
    /// saved places.
    #[default]
    Browse,
    /// One place: its name and what can be done with it (Route, Save, Share).
    Place,
    /// The way to the place: both ends, the modes, the summary, the steps.
    Directions,
}

/// A place the panel or the sheet shows: a search result, a dropped pin, a saved place.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceCard {
    /// Its name as the search gave it; empty for a dropped pin, which the card calls so.
    pub name: String,
    /// What it is, as the key of the words of its kind (`azmaps-kind-city`); empty for nothing.
    pub kind: &'static str,
    /// Where it is, `(lat, lon)`.
    pub at: (f64, f64),
    /// The pin it is, when it is one of the dropped pins (an index into them).
    pub pin: Option<usize>,
}

/// What the panel / sheet holds, and the two flags that go with it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Flow {
    /// The view.
    pub panel: Panel,
    /// The place of [`Panel::Place`]; kept in [`Panel::Directions`], where back returns to it.
    pub place: Option<PlaceCard>,
    /// A desktop's panel is folded away: only its search pill shows.
    pub collapsed: bool,
    /// The map styles' menu is open.
    pub layers_open: bool,
}

impl Flow {
    /// `card` is shown: its view, the panel unfolded, the layers' menu shut.
    pub fn show_place(&mut self, card: PlaceCard) {
        self.place = Some(card);
        self.panel = Panel::Place;
        self.collapsed = false;
        self.layers_open = false;
    }

    /// The way is shown (Route on a place's view, or the first thing asked of an empty search).
    pub fn directions(&mut self) {
        self.panel = Panel::Directions;
        self.collapsed = false;
        self.layers_open = false;
    }

    /// One step back: from the way to its place (when it has one), from a place to the search.
    pub fn back(&mut self) {
        match (self.panel, self.place.is_some()) {
            (Panel::Directions, true) => self.panel = Panel::Place,
            (Panel::Directions | Panel::Place, _) => self.clear(),
            (Panel::Browse, _) => {}
        }
    }

    /// Back to the search: no place, no way.
    pub fn clear(&mut self) {
        self.panel = Panel::Browse;
        self.place = None;
    }

    /// A desktop's panel folds away or comes back.
    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
        self.layers_open = false;
    }

    /// The map styles' menu opens or shuts.
    pub fn toggle_layers(&mut self) {
        self.layers_open = !self.layers_open;
    }

    /// The place's pin was removed (or the pins were cleared): a card of that pin goes with it.
    pub fn pin_removed(&mut self, index: Option<usize>) {
        let gone = self.place.as_ref().is_some_and(|p| match (p.pin, index) {
            (Some(pin), Some(removed)) => pin == removed,
            (Some(_), None) => true,
            _ => false,
        });
        if gone {
            self.clear();
        } else if let (Some(removed), Some(card)) = (index, self.place.as_mut()) {
            // The pins after the removed one moved down by one.
            if let Some(pin) = card.pin.as_mut() {
                if *pin > removed {
                    *pin -= 1;
                }
            }
        }
    }
}

/// One chip under the search bar: its name and what it searches for, both keys of the resources
/// (the words that name a kind of place differ by language), and its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chip {
    /// The chip's name (`azmaps-chip-food`): its label and its accessible name.
    pub name: &'static str,
    /// The words the search is for (`azmaps-chip-food-query`).
    pub query: &'static str,
    /// The icon, a Material name.
    pub icon: &'static str,
}

/// The chips, in the order they stand.
pub static CHIPS: [Chip; 5] = [
    Chip { name: "azmaps-chip-food", query: "azmaps-chip-food-query", icon: "restaurant" },
    Chip { name: "azmaps-chip-coffee", query: "azmaps-chip-coffee-query", icon: "local_cafe" },
    Chip { name: "azmaps-chip-fuel", query: "azmaps-chip-fuel-query", icon: "local_gas_station" },
    Chip { name: "azmaps-chip-parking", query: "azmaps-chip-parking-query", icon: "local_parking" },
    Chip { name: "azmaps-chip-hotels", query: "azmaps-chip-hotels-query", icon: "hotel" },
];

/// The map styles the layers' menu offers: the name (a key of the resources), the value the
/// settings keep, and the style itself, by its value in the settings (the widget's `MapTheme` is
/// chosen where the map is built).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleChoice {
    /// The style's name (`azmaps-style-standard`).
    pub name: &'static str,
    /// What the settings keep.
    pub key: &'static str,
}

/// The styles, in the order the menu lists them; the first is the default.
pub static STYLES: [StyleChoice; 4] = [
    StyleChoice { name: "azmaps-style-standard", key: "standard" },
    StyleChoice { name: "azmaps-style-muted", key: "muted" },
    StyleChoice { name: "azmaps-style-colourful", key: "colourful" },
    StyleChoice { name: "azmaps-style-blue-sea", key: "blue-sea" },
];

/// The settings key of the chosen map style.
pub const STYLE_KEY: &str = "map-style";

/// The index in [`STYLES`] of the style the settings call `key`; the default when it names none.
#[must_use]
pub fn style_index(key: Option<&str>) -> usize {
    key.and_then(|k| STYLES.iter().position(|s| s.key == k)).unwrap_or(0)
}

/// A link to a place on OpenStreetMap, the map AzMaps draws: what Share puts on the clipboard.
#[must_use]
pub fn share_link(lat: f64, lon: f64) -> String {
    format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=16/{lat:.5}/{lon:.5}")
}

#[cfg(test)]
#[path = "flow_test.rs"]
mod flow_test;
