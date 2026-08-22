//! Built-in map themes: MapCSS sheets for the vector-tile renderer
//! (`azul_dll::desktop::extra::map::svg`), one per [`MapTheme`] preset.
//!
//! THE DIALECT the renderer understands is deliberately small — `selector
//! { fill: …; stroke: …; stroke-width: …; }` where the selector is an
//! OpenMapTiles LAYER name (`water`, `building`, `transportation`, …), a
//! `layer.class` pair (`transportation.motorway`) or `canvas` (the tile's
//! base colour, i.e. land). Every colour is a plain hex so the SVG
//! rasteriser never has to parse `hsl()`.
//!
//! # Provenance and licences (credit is REQUIRED, see `MapTheme::credit`)
//!
//! * `POSITRON`, `DARK`, `BRIGHT`, `LIBERTY` are palette extractions from
//!   the styles OpenFreeMap publishes at `tiles.openfreemap.org/styles/*`
//!   (repo `hyperknot/openfreemap-styles`, MIT). The styles themselves:
//!   Positron and Dark (Dark Matter) — CARTO basemaps designed by Stamen
//!   and Paul Norman, forked via `openmaptiles/positron-gl-style` and
//!   `openmaptiles/dark-matter-gl-style`; Bright — `openmaptiles/
//!   osm-bright-gl-style`; Liberty — `maputnik/osm-liberty`. Each is
//!   BSD-3-Clause (code) and CC BY 4.0 (design); the design licence is
//!   what applies to a palette, and CC BY 4.0 permits this use WITH
//!   attribution — which the widget shows in its attribution line.
//! * `GOOGLE_NIGHT` is the "Night mode" style array from the Google Maps
//!   Platform documentation (`developers.google.com/maps/documentation/
//!   javascript/examples/style-array`), whose code samples are Apache-2.0.
//! * `GOOGLE_LIGHT`, `APPLE_LIGHT`, `APPLE_DARK` are AUTHORED here to
//!   resemble the familiar default looks of those products; no asset,
//!   style file or trademark of theirs is used — colour values only.

/// CARTO Positron via OpenFreeMap — the light, desaturated reference look.
pub const POSITRON: &str = r"
canvas { fill: #f2f3f0; }
water { fill: #c2c8ca; stroke: none; }
waterway { fill: none; stroke: #c2c8ca; stroke-width: 0.6; }
park { fill: #e6e9e5; stroke: none; }
landcover { fill: #e6e9e5; stroke: none; }
landuse { fill: #eaeae6; stroke: none; }
building { fill: #eaeae5; stroke: #dbdbda; stroke-width: 0.3; }
transportation { fill: none; stroke: #e0e0e0; stroke-width: 0.8; }
transportation.tertiary { fill: none; stroke: #ffffff; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #ffffff; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #ffffff; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #ffffff; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #ffffff; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #d5d5d5; stroke-width: 0.6; }
boundary { fill: none; stroke: #b3b3b3; stroke-width: 0.6; }
aeroway { fill: #e9e9e6; stroke: none; }
";

/// CARTO Dark Matter via OpenFreeMap — the dark reference look.
pub const DARK: &str = r"
canvas { fill: #0c0c0c; }
water { fill: #1b1b1d; stroke: none; }
waterway { fill: none; stroke: #1b1b1d; stroke-width: 0.6; }
park { fill: #202020; stroke: none; }
landcover { fill: #1a1a1a; stroke: none; }
landuse { fill: #0d0c0c; stroke: none; }
building { fill: #0a0a0a; stroke: #1b1b1d; stroke-width: 0.3; }
transportation { fill: none; stroke: #222222; stroke-width: 0.8; }
transportation.tertiary { fill: none; stroke: #2a2a2a; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #2c2c2c; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #2e2e2e; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #303030; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #333333; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #1f1f1f; stroke-width: 0.6; }
boundary { fill: none; stroke: #3b3b3b; stroke-width: 0.6; }
aeroway { fill: #161616; stroke: none; }
";

/// OSM Bright via OpenFreeMap — the colourful general-purpose look.
pub const BRIGHT: &str = r"
canvas { fill: #f8f4f0; }
water { fill: #aecfe2; stroke: none; }
waterway { fill: none; stroke: #aecfe2; stroke-width: 0.8; }
park { fill: #d8e8c8; stroke: none; }
landcover { fill: #d8e8c8; stroke: none; }
landcover.wood { fill: #c3e6a9; stroke: none; }
landuse { fill: #f2eee9; stroke: none; }
building { fill: #f2eae2; stroke: #dfdbd7; stroke-width: 0.3; }
transportation { fill: none; stroke: #ffffff; stroke-width: 0.9; }
transportation.tertiary { fill: none; stroke: #ffeeaa; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #ffeeaa; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #ffeeaa; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #ffeeaa; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #ffcc88; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #bbbbbb; stroke-width: 0.6; }
boundary { fill: none; stroke: #a4a2ae; stroke-width: 0.7; }
aeroway { fill: #e8e8e8; stroke: none; }
";

/// OSM Liberty via OpenFreeMap — the Bright lineage with a brighter sea.
pub const LIBERTY: &str = r"
canvas { fill: #f8f4f0; }
water { fill: #9ebdff; stroke: none; }
waterway { fill: none; stroke: #9ebdff; stroke-width: 0.8; }
park { fill: #d8e8c8; stroke: none; }
landcover { fill: #b0d59a; stroke: none; }
landcover.wood { fill: #c3e6a9; stroke: none; }
landuse { fill: #dbd8d8; stroke: none; }
building { fill: #dcd9d6; stroke: #d0ccc7; stroke-width: 0.3; }
transportation { fill: none; stroke: #ffffff; stroke-width: 0.9; }
transportation.tertiary { fill: none; stroke: #ffeeaa; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #ffeeaa; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #ffeeaa; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #ffeeaa; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #ffcc88; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #bbbbbb; stroke-width: 0.6; }
boundary { fill: none; stroke: #686869; stroke-width: 0.7; }
aeroway { fill: #e8e8e8; stroke: none; }
";

/// Google Maps' published "Night mode" style array (Apache-2.0 sample).
pub const GOOGLE_NIGHT: &str = r"
canvas { fill: #242f3e; }
water { fill: #17263c; stroke: none; }
waterway { fill: none; stroke: #17263c; stroke-width: 0.8; }
park { fill: #263c3f; stroke: none; }
landcover { fill: #263c3f; stroke: none; }
landuse { fill: #242f3e; stroke: none; }
building { fill: #2b3646; stroke: #212a37; stroke-width: 0.3; }
transportation { fill: none; stroke: #38414e; stroke-width: 0.9; }
transportation.tertiary { fill: none; stroke: #38414e; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #38414e; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #38414e; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #746855; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #746855; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #2f3948; stroke-width: 0.7; }
boundary { fill: none; stroke: #3e4a5c; stroke-width: 0.7; }
aeroway { fill: #2f3948; stroke: none; }
";

/// A Google-Maps-like light look (authored here; colour values only).
pub const GOOGLE_LIGHT: &str = r"
canvas { fill: #f2efe9; }
water { fill: #aadaff; stroke: none; }
waterway { fill: none; stroke: #aadaff; stroke-width: 0.8; }
park { fill: #c9e8b5; stroke: none; }
landcover { fill: #d5ecc2; stroke: none; }
landuse { fill: #ede9e2; stroke: none; }
building { fill: #e8e6e1; stroke: #dcd9d3; stroke-width: 0.3; }
transportation { fill: none; stroke: #ffffff; stroke-width: 0.9; }
transportation.tertiary { fill: none; stroke: #ffffff; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #ffffff; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #fdf3c4; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #f9d776; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #f9d776; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #d4d2cd; stroke-width: 0.6; }
boundary { fill: none; stroke: #c9c6be; stroke-width: 0.7; }
aeroway { fill: #e6e4df; stroke: none; }
";

/// An Apple-Maps-like light look (authored here; colour values only).
pub const APPLE_LIGHT: &str = r"
canvas { fill: #f5f3ef; }
water { fill: #a7d3ee; stroke: none; }
waterway { fill: none; stroke: #a7d3ee; stroke-width: 0.8; }
park { fill: #c6e5b4; stroke: none; }
landcover { fill: #d3ebc2; stroke: none; }
landuse { fill: #eeebe5; stroke: none; }
building { fill: #e7e3dc; stroke: #d9d5cd; stroke-width: 0.3; }
transportation { fill: none; stroke: #ffffff; stroke-width: 0.9; }
transportation.tertiary { fill: none; stroke: #ffffff; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #ffffff; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #fbf1c4; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #f9d578; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #f9d578; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #d2cfc8; stroke-width: 0.6; }
boundary { fill: none; stroke: #cfcac0; stroke-width: 0.7; }
aeroway { fill: #e9e6e0; stroke: none; }
";

/// An Apple-Maps-like dark look (authored here; colour values only).
pub const APPLE_DARK: &str = r"
canvas { fill: #1c1c1e; }
water { fill: #0f2236; stroke: none; }
waterway { fill: none; stroke: #0f2236; stroke-width: 0.8; }
park { fill: #1e2f22; stroke: none; }
landcover { fill: #1b2a1f; stroke: none; }
landuse { fill: #222224; stroke: none; }
building { fill: #2a2a2d; stroke: #353538; stroke-width: 0.3; }
transportation { fill: none; stroke: #2f2f33; stroke-width: 0.9; }
transportation.tertiary { fill: none; stroke: #343438; stroke-width: 1.2; }
transportation.secondary { fill: none; stroke: #383840; stroke-width: 1.4; }
transportation.primary { fill: none; stroke: #4a4633; stroke-width: 1.6; }
transportation.trunk { fill: none; stroke: #5c5133; stroke-width: 1.8; }
transportation.motorway { fill: none; stroke: #5c5133; stroke-width: 2.0; }
transportation.rail { fill: none; stroke: #2b2b2f; stroke-width: 0.7; }
boundary { fill: none; stroke: #45454a; stroke-width: 0.7; }
aeroway { fill: #26262a; stroke: none; }
";

/// The `canvas { fill: #rrggbb; }` colour of a sheet, if it declares one.
#[must_use]
pub fn canvas_fill(sheet: &str) -> Option<[u8; 3]> {
    let start = sheet.find("canvas")?;
    let rest = &sheet[start..];
    let open = rest.find('{')?;
    let close = rest.find('}')?;
    if close < open {
        return None;
    }
    let body = &rest[open + 1..close];
    let fill = body.split(';').find_map(|decl| {
        let (prop, val) = decl.split_once(':')?;
        let prop = prop.trim().to_ascii_lowercase();
        (prop == "fill" || prop == "fill-color").then(|| val.trim())
    })?;
    parse_hex_rgb(fill)
}

/// `#rrggbb` → channels; anything else → `None`.
#[must_use]
pub fn parse_hex_rgb(s: &str) -> Option<[u8; 3]> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Perceived lightness of an sRGB colour, 0 (black) .. 1 (white).
#[must_use]
pub fn luma(rgb: [u8; 3]) -> f32 {
    (0.2126 * f32::from(rgb[0]) + 0.7152 * f32::from(rgb[1]) + 0.0722 * f32::from(rgb[2])) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [(&str, &str, bool); 8] = [
        ("positron", POSITRON, false),
        ("dark", DARK, true),
        ("bright", BRIGHT, false),
        ("liberty", LIBERTY, false),
        ("google-night", GOOGLE_NIGHT, true),
        ("google-light", GOOGLE_LIGHT, false),
        ("apple-light", APPLE_LIGHT, false),
        ("apple-dark", APPLE_DARK, true),
    ];

    #[test]
    fn every_preset_styles_the_layers_the_renderer_draws_with_hex_colours_only() {
        for (name, sheet, _) in ALL {
            for key in ["canvas", "water", "park", "building", "transportation", "transportation.motorway", "boundary"] {
                assert!(
                    sheet.contains(&format!("\n{key} {{")),
                    "{name}: no rule for `{key}`"
                );
            }
            // hex only: the SVG rasteriser must never be handed hsl()/rgb()
            for decl in sheet.split(';') {
                if let Some((prop, val)) = decl.split_once(':') {
                    let prop = prop.trim();
                    let val = val.trim();
                    if prop == "fill" || prop == "stroke" {
                        assert!(
                            val == "none" || parse_hex_rgb(val).is_some(),
                            "{name}: `{prop}: {val}` is not `none` or #rrggbb"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn dark_presets_have_a_dark_canvas_and_light_presets_a_light_one() {
        for (name, sheet, dark) in ALL {
            let canvas = canvas_fill(sheet).unwrap_or_else(|| panic!("{name}: no canvas fill"));
            let l = luma(canvas);
            assert_eq!(l < 0.5, dark, "{name}: canvas luma {l} does not match dark = {dark}");
        }
        assert_eq!(canvas_fill("water { fill: #000000; }"), None);
        assert_eq!(parse_hex_rgb("#0c0c0c"), Some([12, 12, 12]));
        assert_eq!(parse_hex_rgb("rgb(1,2,3)"), None);
    }
}
