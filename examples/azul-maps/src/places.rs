//! The places AzMaps finds by name without a network: the world's capitals and larger cities
//! (`resources/places.tsv`). A search runs on the geocoder worker (`geocode.rs`), never on the
//! UI thread. A name matches without case or accents, in English, in German and as the place
//! calls itself, so `munchen`, `Munich` and `München` all find the same city.

use std::sync::OnceLock;

/// The table as it ships with the app.
const PLACES_TSV: &str = include_str!("../resources/places.tsv");

/// The most suggestions one search gives: what fits under a travel field.
pub const SUGGESTION_LIMIT: usize = 6;

/// A country of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Country {
    pub code: String,
    pub en: String,
    pub de: String,
}

/// A place of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub en: String,
    pub de: String,
    /// The country's code (`DE`), a key into [`Gazetteer::countries`].
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    /// Ranks the places a search finds: the bigger city first.
    pub population: u64,
    /// Every name of the place folded ([`fold`]): English, German and the others.
    keys: Vec<String>,
}

/// A place a search found, with its names and its country's in both languages: the worker does
/// not know the window's language, the window picks the words.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub en: String,
    pub de: String,
    pub country_en: String,
    pub country_de: String,
    pub lat: f64,
    pub lon: f64,
    /// Where it was found: the search results say so.
    pub source: SuggestionSource,
    /// What it is, for its icon.
    pub kind: SuggestionKind,
}

/// Where a suggestion was found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SuggestionSource {
    /// AzMaps' own list of capitals and larger cities.
    #[default]
    Places,
    /// Transitous' search: OpenStreetMap's places and addresses, the timetables' stops.
    Transitous,
    /// A saved list of the user's: its name.
    Saved(String),
}

/// What a suggestion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SuggestionKind {
    #[default]
    City,
    Station,
    Address,
    Place,
    Saved,
}

impl SuggestionKind {
    /// The Material icon of its row.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::City => "location_city",
            Self::Station => "train",
            Self::Address => "home",
            Self::Place => "place",
            Self::Saved => "star",
        }
    }
}

impl Suggestion {
    /// The place's name in German or in English.
    #[must_use]
    pub fn name(&self, german: bool) -> &str {
        if german {
            &self.de
        } else {
            &self.en
        }
    }

    /// What a travel field shows once the place is picked: `Frankfurt am Main, Germany`. A
    /// search for it finds the place again.
    #[must_use]
    pub fn label(&self, german: bool) -> String {
        let country = if german { &self.country_de } else { &self.country_en };
        if country.trim().is_empty() {
            return self.name(german).to_string();
        }
        format!("{}, {country}", self.name(german))
    }
}

/// The parsed table.
#[derive(Debug, Default)]
pub struct Gazetteer {
    pub countries: Vec<Country>,
    pub places: Vec<Place>,
}

/// The table, parsed once by the first search (on the geocoder worker).
pub fn gazetteer() -> &'static Gazetteer {
    static TABLE: OnceLock<Gazetteer> = OnceLock::new();
    TABLE.get_or_init(|| Gazetteer::parse(PLACES_TSV))
}

/// A name as searches compare it: lower case, accents and punctuation gone, single spaces.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let plain = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'ç' | 'ć' | 'č' => "c",
            'ď' | 'đ' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => "i",
            'ł' => "l",
            'ñ' | 'ń' | 'ň' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
            'ŕ' | 'ř' => "r",
            'ś' | 'š' | 'ş' | 'ș' => "s",
            'ß' => "ss",
            'ť' | 'ţ' | 'ț' => "t",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => "u",
            'ý' | 'ÿ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            'æ' => "ae",
            'œ' => "oe",
            // The dot of a lower-cased `İ` and other combining marks.
            '\u{300}'..='\u{36f}' => "",
            c if c.is_alphanumeric() => {
                out.push(c);
                continue;
            }
            _ => " ",
        };
        out.push_str(plain);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Gazetteer {
    /// The table out of its text; a line it cannot read is skipped.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut table = Self::default();
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if let Some(code) = cols[0].strip_prefix('@') {
                if cols.len() == 3 {
                    table.countries.push(Country {
                        code: code.to_string(),
                        en: cols[1].to_string(),
                        de: cols[2].to_string(),
                    });
                }
                continue;
            }
            let [en, de, others, country, lat, lon, population] = cols[..] else {
                continue;
            };
            let (Ok(lat), Ok(lon), Ok(population)) = (lat.parse(), lon.parse(), population.parse())
            else {
                continue;
            };
            let mut keys: Vec<String> = [en, de]
                .into_iter()
                .chain(others.split('|'))
                .map(fold)
                .filter(|k| !k.is_empty())
                .collect();
            keys.dedup();
            table.places.push(Place {
                en: en.to_string(),
                de: de.to_string(),
                country: country.to_string(),
                lat,
                lon,
                population,
                keys,
            });
        }
        table
    }

    fn country(&self, code: &str) -> Option<&Country> {
        self.countries.iter().find(|c| c.code == code)
    }

    /// The places whose name starts like `query`, at most `limit`, the best first. `near` is
    /// where the map looks: of two places that match alike, the nearer big city comes first
    /// (`Be` is Berlin over Frankfurt, Beijing over San Francisco).
    #[must_use]
    pub fn search(&self, query: &str, near: Option<(f64, f64)>, limit: usize) -> Vec<Suggestion> {
        // A picked label (`Frankfurt am Main, Germany`) names its country after the comma.
        let (name, country) = match query.split_once(',') {
            Some((name, country)) => (fold(name), fold(country)),
            None => (fold(query), String::new()),
        };
        if name.is_empty() {
            return Vec::new();
        }
        let word_start = format!(" {name}");
        let mut found: Vec<(u8, f64, &Place)> = Vec::new();
        for place in &self.places {
            // How well a name matches: the whole name, its start, the start of a later word.
            let Some(tier) = place
                .keys
                .iter()
                .filter_map(|key| {
                    if *key == name {
                        Some(0)
                    } else if key.starts_with(&name) {
                        Some(1)
                    } else if key.contains(&word_start) {
                        Some(2)
                    } else {
                        None
                    }
                })
                .min()
            else {
                continue;
            };
            let Some(home) = self.country(&place.country) else {
                continue;
            };
            if !country.is_empty()
                && ![&home.en, &home.de, &home.code]
                    .iter()
                    .any(|n| fold(n).starts_with(&country))
            {
                continue;
            }
            // A big city far away weighs like a small one nearby.
            let weight = match near {
                Some(near) => {
                    let km = crate::model::distance_km(near, (place.lat, place.lon));
                    place.population as f64 / (1.0 + km / 500.0).powi(2)
                }
                None => place.population as f64,
            };
            found.push((tier, weight, place));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        found
            .into_iter()
            .take(limit)
            .filter_map(|(_, _, place)| {
                let home = self.country(&place.country)?;
                Some(Suggestion {
                    en: place.en.clone(),
                    de: place.de.clone(),
                    country_en: home.en.clone(),
                    country_de: home.de.clone(),
                    lat: place.lat,
                    lon: place.lon,
                    source: SuggestionSource::Places,
                    kind: SuggestionKind::City,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(found: &[Suggestion]) -> Vec<&str> {
        found.iter().map(|s| s.en.as_str()).collect()
    }

    #[test]
    fn the_table_reads_every_place_and_every_country_it_names() {
        let table = gazetteer();
        assert!(table.places.len() > 400, "{} places", table.places.len());
        for place in &table.places {
            assert!(table.country(&place.country).is_some(), "{} has no country", place.en);
        }
    }

    #[test]
    fn a_name_is_folded_without_case_accents_or_punctuation() {
        assert_eq!(fold("München"), "munchen");
        assert_eq!(fold("  Saint-Étienne "), "saint etienne");
        assert_eq!(fold("Halle (Saale)"), "halle saale");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("Gießen"), "giessen");
    }

    #[test]
    fn typing_frankfurt_suggests_frankfurt_am_main_first() {
        let found = gazetteer().search("Frankfurt", None, SUGGESTION_LIMIT);
        assert_eq!(names(&found).first(), Some(&"Frankfurt am Main"));
        let frankfurt = &found[0];
        assert!((frankfurt.lat - 50.11).abs() < 0.05 && (frankfurt.lon - 8.68).abs() < 0.05);
        assert_eq!(frankfurt.label(false), "Frankfurt am Main, Germany");
        assert_eq!(frankfurt.label(true), "Frankfurt am Main, Deutschland");
    }

    #[test]
    fn a_city_is_found_by_its_english_or_german_name_with_or_without_accents() {
        for query in ["Munich", "München", "munchen", "Muenchen", "mün"] {
            let found = gazetteer().search(query, None, SUGGESTION_LIMIT);
            assert_eq!(names(&found).first(), Some(&"Munich"), "{query}");
        }
        assert_eq!(names(&gazetteer().search("Praha", None, 3)).first(), Some(&"Prague"));
        assert_eq!(names(&gazetteer().search("Breslau", None, 3)).first(), Some(&"Wrocław"));
    }

    #[test]
    fn the_bigger_nearer_city_comes_first_among_places_that_start_alike() {
        let frankfurt = Some((50.11, 8.68));
        let found = gazetteer().search("Be", frankfurt, SUGGESTION_LIMIT);
        assert_eq!(names(&found).first(), Some(&"Berlin"));
        assert_eq!(found.len(), SUGGESTION_LIMIT);
        let far_away = gazetteer().search("Be", Some((37.77, -122.42)), SUGGESTION_LIMIT);
        assert_eq!(names(&far_away).first(), Some(&"Beijing"));
        let exact = gazetteer().search("Bern", frankfurt, SUGGESTION_LIMIT);
        assert_eq!(names(&exact).first(), Some(&"Bern"), "an exact name beats a longer one");
    }

    #[test]
    fn a_picked_label_finds_its_place_again_and_its_country_must_match() {
        let found = gazetteer().search("Frankfurt am Main, Germany", None, SUGGESTION_LIMIT);
        assert_eq!(names(&found), vec!["Frankfurt am Main"]);
        assert_eq!(names(&gazetteer().search("Wien, Österreich", None, 3)), vec!["Vienna"]);
        assert!(gazetteer().search("Berlin, France", None, 3).is_empty());
    }

    #[test]
    fn nothing_is_suggested_for_an_empty_query_or_a_place_that_is_not_in_the_table() {
        assert!(gazetteer().search("   ", None, SUGGESTION_LIMIT).is_empty());
        assert!(gazetteer().search("Xqzzyville", None, SUGGESTION_LIMIT).is_empty());
    }
}
