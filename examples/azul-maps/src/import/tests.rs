//! The imports and exports, on synthetic files: public landmarks and stations, no personal data.

use std::path::Path;

use super::{
    detect, gpx, import_bytes, import_path, kml, links, review, stem, table, takeout, Format,
    Imported, Unplaced,
};
use crate::saved::{ListKind, PlaceList, SavedPlace};

fn names(list: &PlaceList) -> Vec<&str> {
    list.places.iter().map(|p| p.name.as_str()).collect()
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

const GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <metadata><name>Berlin walk</name></metadata>
  <wpt lat="52.5162746" lon="13.3777041"><name>Brandenburger Tor</name><desc>start here</desc></wpt>
  <wpt lat="52.5186" lon="13.3761"><name>Reichstag</name></wpt>
  <wpt lat="52.52" lon="13.40"></wpt>
  <rte><name>To the Dom</name><rtept lat="52.5163" lon="13.3777"/><rtept lat="52.5191" lon="13.4010"/></rte>
  <trk><name>Recorded</name><trkseg><trkpt lat="52.5" lon="13.3"/><trkpt lat="52.51" lon="13.31"/></trkseg><trkseg><trkpt lat="52.52" lon="13.32"/></trkseg></trk>
</gpx>"#;

#[test]
fn gpx_waypoints_are_places_and_routes_and_tracks_are_lines() {
    let imported = gpx::parse_gpx(GPX, "walk").expect("a GPX file");
    assert_eq!(imported.lists.len(), 1);
    let list = &imported.lists[0];
    assert_eq!(list.name, "Berlin walk", "the file's own name");
    assert_eq!(names(list), ["Brandenburger Tor", "Reichstag", ""]);
    assert!(near(list.places[0].at, (52.516_274_6, 13.377_704_1)));
    assert_eq!(list.places[0].note, "start here");
    assert_eq!(list.routes.len(), 2);
    assert_eq!(list.routes[0].name, "To the Dom");
    assert_eq!(list.routes[1].points.len(), 3, "a track's segments joined");
    assert!(gpx::parse_gpx("<html/>", "x").is_err());
    assert!(gpx::parse_gpx("not xml", "x").is_err());
}

#[test]
fn osmand_favourite_categories_become_lists() {
    let text = r#"<gpx version="1.1" creator="OsmAnd">
      <wpt lat="51.3455" lon="12.3821"><name>Leipzig Hbf</name><type>Travel</type></wpt>
      <wpt lat="52.5250" lon="13.3691"><name>Berlin Hbf</name><type>Travel</type></wpt>
      <wpt lat="52.5191" lon="13.4010"><name>Berliner Dom</name><type>Sights</type></wpt>
    </gpx>"#;
    let imported = gpx::parse_gpx(text, "favourites").expect("a GPX file");
    let lists: Vec<(&str, usize)> = imported.lists.iter().map(|l| (l.name.as_str(), l.places.len())).collect();
    assert_eq!(lists, [("Travel", 2), ("Sights", 1)]);
}

#[test]
fn an_exported_gpx_reads_back_as_the_same_places() {
    let mut list = PlaceList::new("trip", "Trip", ListKind::Named);
    list.places.push(SavedPlace {
        name: "Café & Bar <Mitte>".to_string(),
        at: (52.5219, 13.4132),
        note: "good coffee".to_string(),
        ..SavedPlace::default()
    });
    list.places.push(SavedPlace {
        name: "Leipzig Hbf".to_string(),
        at: (51.3455, 12.3821),
        ..SavedPlace::default()
    });
    let text = gpx::to_gpx(&[list]);
    assert!(text.contains("&amp;") && text.contains("&lt;Mitte&gt;"), "escaped: {text}");
    let back = gpx::parse_gpx(&text, "export").expect("a GPX file");
    assert_eq!(back.lists.len(), 1);
    assert_eq!(back.lists[0].name, "Trip", "the list is the waypoints' type");
    assert_eq!(names(&back.lists[0]), ["Café & Bar <Mitte>", "Leipzig Hbf"]);
    assert_eq!(back.lists[0].places[0].note, "good coffee");
    assert!(near(back.lists[0].places[1].at, (51.3455, 12.3821)));
}

const KML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2">
<Document>
  <name>Organic Maps bookmarks</name>
  <Placemark><name>Alexanderplatz</name><Point><coordinates>13.4132,52.5219</coordinates></Point></Placemark>
  <Folder>
    <name>Trip</name>
    <Placemark><name>Berliner Dom</name><description><![CDATA[<b>the dome</b>]]></description><Point><coordinates>
      13.4010,52.5191,0
    </coordinates></Point></Placemark>
    <Placemark><name>Walk</name><LineString><coordinates>13.3777,52.5163,0 13.4010,52.5191,0</coordinates></LineString></Placemark>
  </Folder>
</Document>
</kml>"#;

#[test]
fn kml_placemarks_are_listed_by_their_folder() {
    let imported = kml::parse_kml(KML, "bookmarks").expect("a KML file");
    let lists: Vec<&str> = imported.lists.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(lists, ["Organic Maps bookmarks", "Trip"]);
    assert_eq!(names(&imported.lists[0]), ["Alexanderplatz"]);
    assert!(near(imported.lists[0].places[0].at, (52.5219, 13.4132)), "KML is lon,lat");
    let trip = &imported.lists[1];
    assert_eq!(names(trip), ["Berliner Dom"]);
    assert_eq!(trip.places[0].note, "<b>the dome</b>");
    assert_eq!(trip.routes.len(), 1);
    assert_eq!(trip.routes[0].points.len(), 2);
    assert!(kml::parse_kml("<gpx/>", "x").is_err());
}

#[test]
fn a_kmz_is_the_kml_in_its_zip() {
    let bytes = include_bytes!("../../tests/fixtures/bookmarks.kmz");
    let imported = kml::parse_kmz(bytes, "bookmarks").expect("a KMZ file");
    assert_eq!(imported.lists.len(), 1);
    assert_eq!(imported.lists[0].name, "Leipzig");
    assert_eq!(names(&imported.lists[0]), ["Leipzig Hbf", "Augustusplatz"]);
    assert_eq!(imported.lists[0].places[1].note, "Gewandhaus");
    assert!(kml::parse_kmz(b"not a zip", "x").is_err());
}

#[test]
fn a_csv_with_coordinate_columns_is_a_list() {
    let text = "Name,Latitude,Longitude,Description\nBerlin Hbf,52.5250,13.3691,station\n\"Leipzig, Hbf\",51.3455,12.3821,\nbroken,abc,12\n";
    let imported = table::parse_csv(text, "stations").expect("a table");
    assert_eq!(imported.lists[0].name, "stations");
    assert_eq!(names(&imported.lists[0]), ["Berlin Hbf", "Leipzig, Hbf"]);
    assert_eq!(imported.lists[0].places[0].note, "station");
    assert_eq!(imported.skipped, 1, "the broken row");
    let semicolons = "lat;lon;name\n51,0404;13,7320;Dresden Hbf\n";
    let imported = table::parse_csv(semicolons, "dresden").expect("a table");
    assert!(near(imported.lists[0].places[0].at, (51.0404, 13.7320)), "decimal commas too");
    assert!(table::parse_csv("a,b\n1,2\n", "x").is_err(), "no coordinates and no links");
}

#[test]
fn a_google_saved_list_takes_coordinates_from_its_links_and_leaves_the_rest_unplaced() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/takeout/Takeout/Saved/Berlin sights.csv"),
    )
    .expect("the fixture");
    let imported = table::parse_csv(&text, "Berlin sights").expect("a table");
    assert_eq!(imported.lists[0].name, "Berlin sights");
    assert_eq!(names(&imported.lists[0]), ["Reichstagsgebäude", "Alexanderplatz"]);
    assert!(near(imported.lists[0].places[0].at, (52.5186, 13.3761)), "!3d!4d");
    assert!(near(imported.lists[0].places[1].at, (52.5219, 13.4132)), "@lat,lon");
    assert_eq!(imported.lists[0].places[0].note, "Dome visit");
    assert!(imported.lists[0].places[0].url.starts_with("https://www.google.com/maps/place/"));
    assert_eq!(imported.unplaced.len(), 1);
    assert_eq!(imported.unplaced[0].name, "Museumsinsel");
    assert_eq!(imported.unplaced[0].note, "Pergamon, Neues Museum");
    assert_eq!(imported.unplaced[0].list, 0);
}

#[test]
fn takeouts_starred_places_new_and_old_and_its_labeled_places() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/takeout/Takeout");
    let saved = std::fs::read_to_string(root.join("Maps (your places)/Saved Places.json")).expect("the fixture");
    let imported = takeout::parse_saved_places(&saved).expect("starred places");
    assert_eq!(imported.lists[0].name, "Saved Places");
    assert_eq!(names(&imported.lists[0]), ["Brandenburger Tor", "Berliner Dom"]);
    assert_eq!(imported.lists[0].places[0].address, "Pariser Platz, 10117 Berlin, Germany");
    assert_eq!(imported.lists[0].places[0].url, "http://maps.google.com/?cid=10001");
    assert!(imported.lists[0].places[0].saved_s > 0);
    assert_eq!(imported.unplaced.len(), 1, "exported without coordinates");
    assert_eq!(imported.unplaced[0].name, "Museumsinsel");
    let old = r#"{"type":"FeatureCollection","features":[{"type":"Feature",
        "geometry":{"type":"Point","coordinates":[12.3821,51.3455]},
        "properties":{"Title":"Leipzig Hbf","Google Maps URL":"http://maps.google.com/?cid=7",
        "Location":{"Address":"Willy-Brandt-Platz 5, Leipzig","Business Name":"Leipzig Hauptbahnhof",
        "Geo Coordinates":{"Latitude":"51.3455","Longitude":"12.3821"}},"Published":"2019-01-01T00:00:00Z"}}]}"#;
    let imported = takeout::parse_saved_places(old).expect("starred places");
    assert_eq!(names(&imported.lists[0]), ["Leipzig Hbf"]);
    assert_eq!(imported.lists[0].places[0].address, "Willy-Brandt-Platz 5, Leipzig");
    let labeled = std::fs::read_to_string(root.join("Maps/My labeled places/Labeled places.json")).expect("the fixture");
    let imported = takeout::parse_labeled_places(&labeled).expect("labeled places");
    assert_eq!(imported.lists[0].name, "Labeled places");
    assert_eq!(names(&imported.lists[0]), ["Work"]);
    assert_eq!(takeout::takeout_file("Saved Places.json"), Some(takeout::TakeoutFile::SavedPlaces));
    assert_eq!(takeout::takeout_file("Labeled places.json"), Some(takeout::TakeoutFile::LabeledPlaces));
    assert_eq!(takeout::takeout_file("other.json"), None);
}

#[test]
fn an_unpacked_takeout_folder_is_read_whole() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/takeout");
    let imported = import_path(&root).expect("a Takeout");
    let mut lists: Vec<&str> = imported.lists.iter().map(|l| l.name.as_str()).collect();
    lists.sort_unstable();
    assert_eq!(lists, ["Berlin sights", "Labeled places", "Saved Places", "Want to go"]);
    assert_eq!(imported.places(), 3 + 1 + 3 + 1);
    let unplaced: Vec<&str> = imported.unplaced.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(unplaced, ["Museumsinsel", "Museumsinsel"]);
    for u in &imported.unplaced {
        let list = &imported.lists[u.list];
        assert!(list.name == "Saved Places" || list.name == "Berlin sights", "{}", list.name);
    }
}

#[test]
fn a_file_is_known_by_its_name_or_its_first_bytes() {
    assert_eq!(detect("walk.gpx", b""), Some(Format::Gpx));
    assert_eq!(detect("x.KML", b""), Some(Format::Kml));
    assert_eq!(detect("x.kmz", b""), Some(Format::Kmz));
    assert_eq!(detect("x.geojson", b""), Some(Format::GeoJson));
    assert_eq!(detect("Saved Places.json", b"{}"), Some(Format::GeoJson));
    assert_eq!(detect("list.csv", b""), Some(Format::Csv));
    assert_eq!(detect("noname", b"<?xml version=\"1.0\"?><gpx>"), Some(Format::Gpx));
    assert_eq!(detect("noname", b"<kml xmlns="), Some(Format::Kml));
    assert_eq!(detect("noname", b"PK\x03\x04"), Some(Format::Kmz));
    assert_eq!(detect("noname", b"{\"type\":\"FeatureCollection\"}"), Some(Format::GeoJson));
    assert_eq!(detect("photo.jpg", b"\xff\xd8"), None);
    assert_eq!(stem("/a/b/My Trip.gpx"), "My Trip");
    assert_eq!(stem("C:\\x\\bookmarks.kmz"), "bookmarks");
    let imported = import_bytes("Berlin.geojson", br#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[13.4,52.5]},"properties":{"name":"x"}}]}"#)
        .expect("GeoJSON");
    assert_eq!(imported.lists[0].name, "Berlin");
    assert!(import_bytes("photo.jpg", b"\xff\xd8").is_err());
}

#[test]
fn extending_an_import_keeps_its_unplaced_places_in_their_lists() {
    let mut a = Imported {
        lists: vec![PlaceList::new("a", "A", ListKind::Named)],
        unplaced: vec![Unplaced { list: 0, name: "x".to_string(), ..Unplaced::default() }],
        skipped: 1,
    };
    let b = Imported {
        lists: vec![PlaceList::new("b", "B", ListKind::Named), PlaceList::new("c", "C", ListKind::Named)],
        unplaced: vec![Unplaced { list: 1, name: "y".to_string(), ..Unplaced::default() }],
        skipped: 2,
    };
    a.extend(b);
    assert_eq!(a.lists.len(), 3);
    assert_eq!(a.unplaced[1].list, 2, "C moved to index 2");
    assert_eq!(a.skipped, 3);
}

#[test]
fn coordinates_and_names_come_out_of_google_maps_links() {
    use links::{google_coordinates, google_place_name};
    let at = |url| google_coordinates(url);
    assert_eq!(at("https://www.google.com/maps/place/X/@52.1,13.1,17z/data=!3m1!4b1!4m5!3m4!1s0x0:0x1!8m2!3d52.5186!4d13.3761"), Some((52.5186, 13.3761)), "the place's own over the view's");
    assert_eq!(at("https://www.google.com/maps/@52.5219,13.4132,15z"), Some((52.5219, 13.4132)));
    assert_eq!(at("https://www.google.com/maps/search/51.3455,12.3821"), Some((51.3455, 12.3821)));
    assert_eq!(at("https://www.google.com/maps/search/51.3455,+12.3821?entry=tts"), Some((51.3455, 12.3821)));
    assert_eq!(at("https://maps.google.com/?q=52.5,13.4"), Some((52.5, 13.4)));
    assert_eq!(at("https://maps.google.com/maps?ll=-33.8688,151.2093&z=12"), Some((-33.8688, 151.2093)));
    assert_eq!(at("http://maps.google.com/?cid=10001"), None);
    assert_eq!(at("https://www.google.com/maps/place/Museumsinsel/data=!4m2!3m1!1s0x47:0x12"), None);
    assert_eq!(at("https://www.google.com/maps/@95.0,13.4,15z"), None, "not a place on Earth");
    assert_eq!(google_place_name("https://www.google.com/maps/place/Reichstagsgeb%C3%A4ude/data=!4m2").as_deref(), Some("Reichstagsgebäude"));
    assert_eq!(google_place_name("https://www.google.com/maps/place/Brandenburger+Tor/@52.5,13.3,17z").as_deref(), Some("Brandenburger Tor"));
    assert_eq!(google_place_name("https://www.google.com/maps/@52.5,13.3,17z"), None);
}

#[test]
fn a_shared_apple_maps_link_names_its_place() {
    use links::{apple_maps_link, percent_decode, shared_link};
    let place = apple_maps_link("https://maps.apple.com/?ll=52.516275,13.377704&q=Brandenburger%20Tor").expect("a place");
    assert_eq!(place.name, "Brandenburger Tor");
    assert_eq!(place.at, Some((52.516_275, 13.377_704)));
    let place = apple_maps_link("https://maps.apple.com/place?coordinate=51.3455,12.3821&name=Leipzig%20Hbf").expect("a place");
    assert_eq!((place.name.as_str(), place.at), ("Leipzig Hbf", Some((51.3455, 12.3821))));
    let place = apple_maps_link("https://maps.apple.com/?address=Pariser%20Platz,%2010117%20Berlin&q=Brandenburger+Tor").expect("a place");
    assert_eq!(place.at, None, "no coordinates: looked up by name and address");
    assert_eq!(place.address, "Pariser Platz, 10117 Berlin");
    assert_eq!(apple_maps_link("https://example.org/?ll=1,2"), None);
    let google = shared_link("https://www.google.com/maps/place/Alexanderplatz/@52.5219,13.4132,17z").expect("a place");
    assert_eq!((google.name.as_str(), google.at), ("Alexanderplatz", Some((52.5219, 13.4132))));
    assert_eq!(shared_link("not a link"), None);
    assert_eq!(percent_decode("Caf%C3%A9+Einstein%2C%20Berlin"), "Café Einstein, Berlin");
    assert_eq!(percent_decode("100%"), "100%", "a lone percent stays");
}

#[test]
fn a_single_clear_match_is_taken_and_anything_less_is_reviewed() {
    use review::{query_of, verdict, Candidate, Verdict};
    let c = |label: &str| Candidate { label: label.to_string(), at: (52.5, 13.4) };
    assert_eq!(verdict("Museumsinsel", &[c("Museumsinsel, Berlin"), c("Museumsinsel Bhf, Berlin")]), Verdict::Certain(0));
    assert_eq!(verdict("museumsinsel", &[c("Museumsinsel, Berlin")]), Verdict::Certain(0), "without case");
    assert_eq!(verdict("Brandenburger Tor", &[c("Brandenburger Tor, Berlin"), c("Brandenburger Tor, Potsdam")]), Verdict::Uncertain, "two of that name");
    assert_eq!(verdict("Café Einstein", &[c("Einstein Kaffee, Berlin")]), Verdict::Uncertain);
    assert_eq!(verdict("Atlantis", &[]), Verdict::NotFound);
    let u = Unplaced { name: "Museumsinsel".to_string(), address: "Bodestraße, Berlin".to_string(), ..Unplaced::default() };
    assert_eq!(query_of(&u), "Museumsinsel, Bodestraße, Berlin");
    assert_eq!(query_of(&Unplaced { name: "Museumsinsel".to_string(), ..Unplaced::default() }), "Museumsinsel");
}
