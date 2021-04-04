#![cfg(feature = "font_loading")]

use azul_css::{U8Vec, AzString};
use rust_fontconfig::FcFontCache;

// serif
#[cfg(target_os = "windows")]
const KNOWN_SYSTEM_SERIF_FONTS: &[&str] = &[
    "Times New Roman"
];
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
const KNOWN_SYSTEM_SERIF_FONTS: &[&str] = &[
    "Times",
    "New York",
    "Palatino",
];


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
    "Tahoma", // XP
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
    "San Francisco", // default on El Capitan and newer
    "Helvetica Neue", // default on Yosemite
    "Lucida Grande", // other
];


// italic / oblique / fantasy: same as sans-serif for now, but set the oblique flag

/// Returns the font file contents from the computer + the font index
pub fn load_system_font(id: &str, fc_cache: &FcFontCache) -> Option<(U8Vec, i32)> {
    use rust_fontconfig::{FcPattern, FcFontPath, PatternMatch};

    let mut patterns = Vec::new();

    match id {
        "monospace" => {
            #[cfg(target_os = "linux")] {
                if let Some(gsettings_pref) = linux_get_gsettings_font("monospace-font-name") {
                    patterns.push(FcPattern {
                        name: Some(gsettings_pref),
                        monospace: PatternMatch::True,
                        .. FcPattern::default()
                    });
                }
                if let Some(fontconfig_pref) = linux_get_fc_match_font("monospace") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        monospace: PatternMatch::True,
                        .. FcPattern::default()
                    });
                }
            }

            for monospace_font_name in KNOWN_SYSTEM_MONOSPACE_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(monospace_font_name.to_string()),
                    monospace: PatternMatch::True,
                    .. FcPattern::default()
                });
            }

            patterns.push(FcPattern {
                monospace: PatternMatch::True,
                .. FcPattern::default()
            });
        },
        "fantasy" | "oblique" => {
            #[cfg(target_os = "linux")] {
                if let Some(fontconfig_pref) = linux_get_fc_match_font("sans-serif") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        oblique: PatternMatch::True,
                        .. FcPattern::default()
                    });
                }
            }
            for serif_font in KNOWN_SYSTEM_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(serif_font.to_string()),
                    oblique: PatternMatch::True,
                    .. FcPattern::default()
                });
            }

            patterns.push(FcPattern {
                oblique: PatternMatch::True,
                .. FcPattern::default()
            });
        },
        "italic" => {
            #[cfg(target_os = "linux")] {
                if let Some(fontconfig_pref) = linux_get_fc_match_font("italic") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        italic: PatternMatch::True,
                        .. FcPattern::default()
                    });
                }
            }
            for serif_font in KNOWN_SYSTEM_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(serif_font.to_string()),
                    italic: PatternMatch::True,
                    .. FcPattern::default()
                });
            }

            patterns.push(FcPattern {
                italic: PatternMatch::True,
                .. FcPattern::default()
            });
        },
        "sans-serif" => {
            #[cfg(target_os = "linux")] {
                if let Some(gsettings_pref) = linux_get_gsettings_font("font-name") {
                    patterns.push(FcPattern {
                        name: Some(gsettings_pref),
                        .. FcPattern::default()
                    });
                }
                if let Some(fontconfig_pref) = linux_get_fc_match_font("sans-serif") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        .. FcPattern::default()
                    });
                }
            }

            for sans_serif_font in KNOWN_SYSTEM_SANS_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(sans_serif_font.to_string()),
                    .. FcPattern::default()
                });
            }
        },
        "serif" => {
            #[cfg(target_os = "linux")] {
                if let Some(fontconfig_pref) = linux_get_fc_match_font("serif") {
                    patterns.push(FcPattern {
                        name: Some(fontconfig_pref),
                        .. FcPattern::default()
                    });
                }
            }

            for serif_font in KNOWN_SYSTEM_SERIF_FONTS.iter() {
                patterns.push(FcPattern {
                    name: Some(serif_font.to_string()),
                    .. FcPattern::default()
                });
            }
        },
        other => {
            patterns.push(FcPattern {
                name: Some(other.clone().into()),
                .. FcPattern::default()
            });

            patterns.push(FcPattern {
                family: Some(other.clone().into()),
                .. FcPattern::default()
            });
        }
    }

    // always resolve to some font, even if the font is wrong it's better
    // than if the text doesn't show up at all
    patterns.push(FcPattern::default());

    for pattern in patterns {
        if let Some(FcFontPath { path, font_index }) = fc_cache.query(&pattern) {
            use std::fs;
            use std::path::Path;
            if let Ok(bytes) = fs::read(Path::new(path)) {
                return Some((bytes.into(), *font_index as i32));
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
        .ok().map(|output| output.stdout)
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
        .ok().map(|output| output.stdout)
        .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
        .map(|stdout_string| stdout_string.lines().collect::<String>())
        .and_then(|s| Some(parse_fc_match_font(&s)?.to_string()))
}

// parse:
// DejaVuSans.ttf: "DejaVu Sans" "Book"
// DejaVuSansMono.ttf: "DejaVu Sans Mono" "Book"
fn parse_fc_match_font(input: &str) -> Option<&str> {
    use std::char;

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

// Test that the font gets garbage collected correctly
#[test]
fn test_font_gc() {

    use core::{
        collections::BTreeMap,
        hash::Hash,
    };
    use alloc::sync::Arc;
    use azul_core::{
        FastHashMap, FastHashSet,
        ui_description::UiDescription,
        ui_state::UiState,
        app_resources::{
            AppResources, Au, FakeRenderApi,
            scan_ui_description_for_image_keys,
            scan_ui_description_for_font_keys,
            garbage_collect_fonts_and_images,
            add_fonts_and_images, FontMetrics,
        },
        display_list::DisplayList,
    };
    use crate::xml::DomXml;

    fn build_map<T: Hash + Eq, U>(i: Vec<(T, U)>) -> FastHashMap<T, U> {
        let mut map = FastHashMap::default();
        for (k, v) in i { map.insert(k, v); }
        map
    }

    fn build_set<T: Hash + Eq>(i: Vec<T>) -> FastHashSet<T> {
        let mut set = FastHashSet::default();
        for x in i { set.insert(x); }
        set
    }

    fn build_ui(xml: &str, css: &str) -> (UiState<Mock>, UiDescription, DisplayList) {

        use azul_css::from_str as css_from_str;

        let is_mouse_down = false;
        let focused_node = None;
        let hovered_nodes = BTreeMap::new();
        let css = css_from_str(css).unwrap();

        let mut ui_state = UiState::new(DomXml::mock(xml).into_dom(), None);
        let ui_description = UiDescription::new(&mut ui_state, &css, &focused_node, &hovered_nodes, is_mouse_down);
        let display_list = DisplayList::new(&ui_description, &ui_state);

        (ui_state, ui_description, display_list)
    }

    #[cfg(feature = "font_loading")]
    fn fake_load_font_fn(_f: &FontSource) -> Option<LoadedFontSource> {
        Some(LoadedFontSource {
            font_bytes: Vec::new(),
            font_index: 0,
            font_metrics: FontMetrics::zero(),
        })
    }

    #[cfg(feature = "image_loading")]
    fn fake_load_image_font_fn(_i: &ImageSource) -> Option<LoadedImageSource> {
        Some(LoadedImageSource {
            image_bytes_decoded: ImageData::Raw(Arc::new(Vec::new())),
            image_descriptor: ImageDescriptor {
                format: RawImageFormat::R8,
                dimensions: (0, 0),
                stride: None,
                offset: 0,
                is_opaque: true,
                allow_mipmaps: false,
            },
        })
    }

    struct Mock;

    use azul_core::ui_solver::DEFAULT_FONT_SIZE_PX;

    const DEFAULT_FONT_SIZE: f32 = DEFAULT_FONT_SIZE_PX as f32;

    let pipeline_id = PipelineId::new();
    let mut app_resources = AppResources::new();
    app_resources.add_pipeline(pipeline_id);

    let css = r#"
        #one { font-family: Helvetica; }
        #two { font-family: Arial; }
        #three { font-family: Times New Roman; }
    "#;

    let (ui_state_frame_1, _, display_list_frame_1) = build_ui(r#"
        <p id="one">Hello</p>
        <p id="two">Hello</p>
        <p id="three">Hello</p>
    "#, css);

    let (ui_state_frame_2, _, display_list_frame_2) = build_ui(r#"
        <p>Hello</p>
    "#, css);

    let (ui_state_frame_3, _, display_list_frame_3) = build_ui(r#"
        <p id="one">Hello</p>
        <p id="two">Hello</p>
        <p id="three">Hello</p>
    "#, css);

    let node_data_1 = &ui_state_frame_1.get_dom().arena.node_data;
    let node_data_2 = &ui_state_frame_2.get_dom().arena.node_data;
    let node_data_3 = &ui_state_frame_3.get_dom().arena.node_data;

    // Assert that the UI doesn't contain any images
    assert_eq!(scan_ui_description_for_image_keys(&app_resources, &display_list_frame_1, &node_data_1), FastHashSet::default());
    assert_eq!(scan_ui_description_for_image_keys(&app_resources, &display_list_frame_2, &node_data_2), FastHashSet::default());
    assert_eq!(scan_ui_description_for_image_keys(&app_resources, &display_list_frame_3, &node_data_3), FastHashSet::default());

    assert_eq!(scan_ui_description_for_font_keys(&app_resources, &display_list_frame_1, &node_data_1), build_map(vec![
        (ImmediateFontId::Unresolved("Arial".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
        (ImmediateFontId::Unresolved("Helvetica".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
        (ImmediateFontId::Unresolved("Times New Roman".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
    ]));
    assert_eq!(scan_ui_description_for_font_keys(&app_resources, &display_list_frame_2, &node_data_2), build_map(vec![
        (ImmediateFontId::Unresolved("serif".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
    ]));
    assert_eq!(scan_ui_description_for_font_keys(&app_resources, &display_list_frame_3, &node_data_3), build_map(vec![
        (ImmediateFontId::Unresolved("Arial".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
        (ImmediateFontId::Unresolved("Helvetica".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
        (ImmediateFontId::Unresolved("Times New Roman".to_string()), build_set(vec![Au::from_px(DEFAULT_FONT_SIZE)])),
    ]));

    let mut fake_render_api = FakeRenderApi::new();

    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_1, &node_data_1, fake_load_font_fn, fake_load_image_font_fn);
    assert_eq!(app_resources.currently_registered_fonts[&pipeline_id].len(), 3);
    assert_eq!(app_resources.last_frame_font_keys[&pipeline_id].len(), 3);

    // Assert that the first frame doesn't delete the fonts again
    garbage_collect_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id);
    assert_eq!(app_resources.currently_registered_fonts[&pipeline_id].len(), 3);

    // Assert that fonts don't get double-inserted, still the same font sources as previously
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_3, &node_data_3, fake_load_font_fn, fake_load_image_font_fn);
    garbage_collect_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id);
    assert_eq!(app_resources.currently_registered_fonts[&pipeline_id].len(), 3);

    // Assert that no new fonts get added on subsequent frames
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_3, &node_data_3, fake_load_font_fn, fake_load_image_font_fn);
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_3, &node_data_3, fake_load_font_fn, fake_load_image_font_fn);
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_3, &node_data_3, fake_load_font_fn, fake_load_image_font_fn);
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_3, &node_data_3, fake_load_font_fn, fake_load_image_font_fn);
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_3, &node_data_3, fake_load_font_fn, fake_load_image_font_fn);
    garbage_collect_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id);
    assert_eq!(app_resources.currently_registered_fonts[&pipeline_id].len(), 3);

    // If the DOM changes, the fonts should get deleted, the only font still present is "sans-serif"
    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_2, &node_data_2, fake_load_font_fn, fake_load_image_font_fn);
    garbage_collect_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id);
    assert_eq!(app_resources.currently_registered_fonts[&pipeline_id].len(), 1);

    add_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id, &display_list_frame_1, &node_data_1, fake_load_font_fn, fake_load_image_font_fn);
    garbage_collect_fonts_and_images(&mut app_resources, &mut fake_render_api, &pipeline_id);
    assert_eq!(app_resources.currently_registered_fonts[&pipeline_id].len(), 3);
}