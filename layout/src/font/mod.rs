#![cfg(feature = "font_loading")]

use azul_css::{AzString, U8Vec};
use rust_fontconfig::{FcFontCache, FontSource};

pub mod loading;

// serif
#[cfg(target_os = "windows")]
const KNOWN_SYSTEM_SERIF_FONTS: &[&str] = &["Times New Roman"];
#[cfg(target_os = "linux")]
const KNOWN_SYSTEM_SERIF_FONTS: &[&str] = &[
    // <ask fc-match first>
    "Times",
    "Times New Roman",
    "DejaVu Serif",
    "Free Serif",
    "Noto Serif",
    "Bitstream Vera Serif",
    "Roman",
    "Regular",
];
#[cfg(target_os = "macos")]
const KNOWN_SYSTEM_SERIF_FONTS: &[&str] = &["Times", "New York", "Palatino"];

// monospace
#[cfg(target_os = "windows")]
const KNOWN_SYSTEM_MONOSPACE_FONTS: &[&str] = &[
    "Segoe UI Mono",
    "Courier New",
    "Cascadia Code",
    "Cascadia Mono",
];
#[cfg(target_os = "linux")]
const KNOWN_SYSTEM_MONOSPACE_FONTS: &[&str] = &[
    // <ask fc-match first>
    "Source Code Pro",
    "Cantarell",
    "DejaVu Sans Mono",
    "Roboto Mono",
    "Ubuntu Monospace",
    "Droid Sans Mono",
];
#[cfg(target_os = "macos")]
const KNOWN_SYSTEM_MONOSPACE_FONTS: &[&str] = &[
    "SF Mono",
    "Menlo",
    "Monaco",
    "Oxygen Mono",
    "Source Code Pro",
    "Fira Mono",
];

// sans-serif
#[cfg(target_os = "windows")]
const KNOWN_SYSTEM_SANS_SERIF_FONTS: &[&str] = &[
    "Segoe UI", // Vista and newer, including Windows 10
    "Tahoma",   // XP
    "Microsoft Sans Serif",
    "MS Sans Serif",
    "Helv",
];
#[cfg(target_os = "linux")]
const KNOWN_SYSTEM_SANS_SERIF_FONTS: &[&str] = &[
    // <ask fc-match first>
    "Ubuntu",
    "Arial",
    "DejaVu Sans",
    "Noto Sans",
    "Liberation Sans",
];
#[cfg(target_os = "macos")]
const KNOWN_SYSTEM_SANS_SERIF_FONTS: &[&str] = &[
    "San Francisco",  // default on El Capitan and newer
    "Helvetica Neue", // default on Yosemite
    "Lucida Grande",  // other
];

#[cfg(target_family = "wasm")]
const KNOWN_SYSTEM_MONOSPACE_FONTS: &[&str] = &[];
#[cfg(target_family = "wasm")]
const KNOWN_SYSTEM_SANS_SERIF_FONTS: &[&str] = &[];
#[cfg(target_family = "wasm")]
const KNOWN_SYSTEM_SERIF_FONTS: &[&str] = &[];

// italic / oblique / fantasy: same as sans-serif for now, but set the oblique flag

/// Returns the font file contents from the computer + the font index
pub fn load_system_font(id: &str, fc_cache: &FcFontCache) -> Option<(U8Vec, i32)> {
    use rust_fontconfig::{FcFontPath, FcPattern, PatternMatch};

    let mut patterns = Vec::new();

    match id {
        "monospace" => {
            #[cfg(target_os = "linux")]
            {
                if let Some(gsettings_pref) = linux_get_gsettings_font("monospace-font-name") {
                    patterns.push(FcPattern {
                        name: Some(gsettings_pref),
                        ..FcPattern::default()
                    });
                }
                if let Some(fontconfig_pref) = linux_get_fc_match_font("monospace") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        ..FcPattern::default()
                    });
                }
            }

            for monospace_font_name in KNOWN_SYSTEM_MONOSPACE_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(monospace_font_name.to_string()),
                    ..FcPattern::default()
                });
            }

            patterns.push(FcPattern {
                monospace: PatternMatch::True,
                ..FcPattern::default()
            });
        }
        "fantasy" | "oblique" => {
            #[cfg(target_os = "linux")]
            {
                if let Some(fontconfig_pref) = linux_get_fc_match_font("sans-serif") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        oblique: PatternMatch::True,
                        ..FcPattern::default()
                    });
                }
            }
            for serif_font in KNOWN_SYSTEM_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(serif_font.to_string()),
                    oblique: PatternMatch::True,
                    ..FcPattern::default()
                });
            }

            patterns.push(FcPattern {
                oblique: PatternMatch::True,
                ..FcPattern::default()
            });
        }
        "italic" => {
            #[cfg(target_os = "linux")]
            {
                if let Some(fontconfig_pref) = linux_get_fc_match_font("italic") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        italic: PatternMatch::True,
                        ..FcPattern::default()
                    });
                }
            }
            for serif_font in KNOWN_SYSTEM_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(serif_font.to_string()),
                    italic: PatternMatch::True,
                    ..FcPattern::default()
                });
            }

            patterns.push(FcPattern {
                italic: PatternMatch::True,
                ..FcPattern::default()
            });
        }
        "sans-serif" => {
            #[cfg(target_os = "linux")]
            {
                if let Some(gsettings_pref) = linux_get_gsettings_font("font-name") {
                    patterns.push(FcPattern {
                        name: Some(gsettings_pref),
                        ..FcPattern::default()
                    });
                }
                if let Some(fontconfig_pref) = linux_get_fc_match_font("sans-serif") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        ..FcPattern::default()
                    });
                }
            }

            for sans_serif_font in KNOWN_SYSTEM_SANS_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(sans_serif_font.to_string()),
                    ..FcPattern::default()
                });
            }
        }
        "serif" => {
            #[cfg(target_os = "linux")]
            {
                if let Some(fontconfig_pref) = linux_get_fc_match_font("serif") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        ..FcPattern::default()
                    });
                }
            }

            for serif_font in KNOWN_SYSTEM_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(serif_font.to_string()),
                    ..FcPattern::default()
                });
            }
        }
        other => {
            patterns.push(FcPattern {
                name: Some(other.clone().into()),
                ..FcPattern::default()
            });

            patterns.push(FcPattern {
                family: Some(other.clone().into()),
                ..FcPattern::default()
            });
        }
    }

    // always resolve to some font, even if the font is wrong it's better
    // than if the text doesn't show up at all
    patterns.push(FcPattern::default());

    for pattern in patterns {
        // TODO: handle font fallbacks via s.fallbacks
        let font_source = fc_cache
            .query(&pattern, &mut Vec::new())
            .and_then(|s| fc_cache.get_font_by_id(&s.id));

        let font_source = match font_source {
            Some(s) => s,
            None => continue,
        };

        match font_source {
            FontSource::Memory(m) => {
                return Some((m.bytes.clone().into(), m.font_index as i32));
            }
            FontSource::Disk(d) => {
                use std::{fs, path::Path};
                if let Ok(bytes) = fs::read(Path::new(&d.path)) {
                    return Some((bytes.into(), d.font_index as i32));
                }
            }
        }
    }

    None
}

#[cfg(all(target_os = "linux", feature = "std"))]
fn linux_get_gsettings_font(font_name: &'static str) -> Option<String> {
    // Execute "gsettings get org.gnome.desktop.interface font-name" and parse the output
    std::process::Command::new("gsettings")
        .arg("get")
        .arg("org.gnome.desktop.interface")
        .arg(font_name)
        .output()
        .ok()
        .map(|output| output.stdout)
        .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
        .map(|stdout_string| stdout_string.lines().collect::<String>())
        .map(|s| parse_gsettings_font(&s).to_string())
}

fn parse_gsettings_font(input: &str) -> &str {
    use std::char;
    let input = input.trim();
    let input = input.trim_matches('\'');
    let input = input.trim_end_matches(char::is_numeric);
    let input = input.trim();
    input
}

#[cfg(all(target_os = "linux", feature = "std"))]
fn linux_get_fc_match_font(font_name: &'static str) -> Option<String> {
    // Execute "fc-match serif" and parse the output
    std::process::Command::new("fc-match")
        .arg(font_name)
        .output()
        .ok()
        .map(|output| output.stdout)
        .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
        .map(|stdout_string| stdout_string.lines().collect::<String>())
        .and_then(|s| Some(parse_fc_match_font(&s)?.to_string()))
}

// parse:
// DejaVuSans.ttf: "DejaVu Sans" "Book"
// DejaVuSansMono.ttf: "DejaVu Sans Mono" "Book"
fn parse_fc_match_font(input: &str) -> Option<&str> {
    let input = input.trim();
    let mut split_iterator = input.split(":");
    split_iterator.next()?;

    let fonts_str = split_iterator.next()?; // "DejaVu Sans" "Book"
    let fonts_str = fonts_str.trim();
    let mut font_iterator = input.split("\" \"");
    let first_font = font_iterator.next()?; // "DejaVu Sans

    let first_font = first_font.trim();
    let first_font = first_font.trim_start_matches('"');
    let first_font = first_font.trim_end_matches('"');
    let first_font = first_font.trim();

    Some(first_font)
}
