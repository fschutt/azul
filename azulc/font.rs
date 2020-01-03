/// Returns the font file contents from the computer + the font index
pub fn load_system_font(id: &str) -> Option<(Vec<u8>, i32)> {
    use font_loader::system_fonts::{self, FontPropertyBuilder};
    let font_builder = match id {
        "monospace" => {
            #[cfg(target_os = "linux")] {
                let native_monospace_font = linux_get_native_font(LinuxNativeFontType::Monospace);
                FontPropertyBuilder::new().family(&native_monospace_font)
            }
            #[cfg(not(target_os = "linux"))] {
                FontPropertyBuilder::new().monospace()
            }
        },
        "fantasy" => FontPropertyBuilder::new().oblique(),
        "sans-serif" => {
            #[cfg(target_os = "mac_os")] {
                FontPropertyBuilder::new().family("Helvetica")
            }
            #[cfg(target_os = "linux")] {
                let native_sans_serif_font = linux_get_native_font(LinuxNativeFontType::SansSerif);
                FontPropertyBuilder::new().family(&native_sans_serif_font)
            }
            #[cfg(all(not(target_os = "linux"), not(target_os = "mac_os")))] {
                FontPropertyBuilder::new().family("Segoe UI")
            }
        },
        "serif" => {
            FontPropertyBuilder::new().family("Times New Roman")
        },
        other => FontPropertyBuilder::new().family(other)
    };

    system_fonts::get(&font_builder.build())
}

/// Return the native fonts
#[cfg(target_os = "linux")]
pub enum LinuxNativeFontType { SansSerif, Monospace }

#[cfg(target_os = "linux")]
pub fn linux_get_native_font(font_type: LinuxNativeFontType) -> String {

    use std::process::Command;
    use self::LinuxNativeFontType::*;

    let font_name = match font_type {
        SansSerif => "font-name",
        Monospace => "monospace-font-name",
    };

    let fallback_font_name = match font_type {
        SansSerif => "Ubuntu",
        Monospace => "Ubuntu Mono",
    };

    // Execute "gsettings get org.gnome.desktop.interface font-name" and parse the output
    let gsetting_cmd_result =
        Command::new("gsettings")
            .arg("get")
            .arg("org.gnome.desktop.interface")
            .arg(font_name)
            .output()
            .ok().map(|output| output.stdout)
            .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
            .map(|stdout_string| stdout_string.lines().collect::<String>());

    match &gsetting_cmd_result {
        Some(s) => parse_gsettings_font(s).to_string(),
        None => fallback_font_name.to_string(),
    }
}

/// 'Ubuntu Mono 13' => Ubuntu Mono
#[cfg(target_os = "linux")]
pub fn parse_gsettings_font(input: &str) -> &str {
    use std::char;
    let input = input.trim();
    let input = input.trim_matches('\'');
    let input = input.trim_end_matches(char::is_numeric);
    let input = input.trim();
    input
}

#[test]
#[cfg(target_os = "linux")]
fn test_parse_gsettings_font() {
    assert_eq!(parse_gsettings_font("'Ubuntu 11'"), "Ubuntu");
    assert_eq!(parse_gsettings_font("'Ubuntu Mono 13'"), "Ubuntu Mono");
}

#[test]
fn test_font_gc() {

    use std::{
        collections::BTreeMap,
        hash::Hash,
        sync::Arc,
    };
    use azul_core::{
        FastHashMap, FastHashSet,
        ui_description::UiDescription,
        ui_state::UiState,
        app_resources::{
            AppResources, Au, FakeRenderApi,
            LoadFontFn, FontSource, LoadedFontSource,
            LoadImageFn, ImageSource, LoadedImageSource, RawImageFormat, ImageDescriptor, ImageData,
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

        extern crate azul_css_parser;

        let is_mouse_down = false;
        let focused_node = None;
        let hovered_nodes = BTreeMap::new();
        let css = azul_css_parser::new_from_str(css).unwrap();

        let mut ui_state = UiState::new(DomXml::mock(xml).into_dom(), None);
        let ui_description = UiDescription::new(&mut ui_state, &css, &focused_node, &hovered_nodes, is_mouse_down);
        let display_list = DisplayList::new(&ui_description, &ui_state);

        (ui_state, ui_description, display_list)
    }

    let fake_load_font_fn = LoadFontFn(|_f: &FontSource| -> Option<LoadedFontSource> {
        Some(LoadedFontSource {
            font_bytes: Vec::new(),
            font_index: 0,
            font_metrics: FontMetrics::zero(),
        })
    });

    let fake_load_image_font_fn = LoadImageFn(|_i: &ImageSource| -> Option<LoadedImageSource> {
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
    });

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