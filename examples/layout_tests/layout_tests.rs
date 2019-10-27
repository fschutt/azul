//! Runs all the tests in the `/tests` directory

extern crate azul_layout;
extern crate azul_core;
extern crate azul_css_parser;
extern crate azul;

use std::fs;
use azul::{
    xml::{
        self, XmlComponentMap, render_dom_from_app_node_inner,
        XmlNode, FilteredComponentArguments,
    },
    dom::Dom,
    css::Css,
};
use azul_core::{
    display_list::CachedDisplayList,
};

struct Mock { }

fn load_files(dir: &str) -> Vec<(String, String)> {
    fs::read_dir(dir).unwrap().filter_map(|file_name| {
        let file_name = file_name.ok()?;
        let file_name = file_name.path();
        let file_contents = fs::read_to_string(&file_name).ok()?;
        let file_name = file_name.file_name().unwrap().to_string_lossy().to_string();
        Some((file_name.clone(), file_contents))
    }).collect()
}

fn find_root_node<'a>(xml: &'a Vec<XmlNode>, node_type: &str) -> Option<&'a XmlNode> {
    xml.iter().find(|node| node.node_type == node_type)
}

fn find_attribute<'a>(node: &'a XmlNode, attribute: &str) -> Option<&'a str> {
    node.attributes.get(attribute).map(|s| s.as_str())
}

fn get_content<'a>(xml: &'a XmlNode) -> &'a str {
    const DEFAULT_STR: &str = "";
    xml.text.as_ref().map(|s| s.as_str()).unwrap_or(DEFAULT_STR)
}

// Parse a string like "600x100" -> (600, 100)
fn parse_size(output_size: &str) -> Option<(f32, f32)> {
    let output_size = output_size.trim();
    let mut iter = output_size.split("x");
    let w = iter.next()?;
    let h = iter.next()?;
    let w = w.trim();
    let h = h.trim();
    let w = w.parse::<f32>().ok()?;
    let h = h.parse::<f32>().ok()?;
    Some((w, h))
}

fn create_display_list(dom: Dom<Mock>, css: &Css, size: (f32, f32)) -> CachedDisplayList {

    use std::{rc::Rc, collections::BTreeMap};
    use azul_core::{
        app_resources::{
            AppResources, Epoch, FakeRenderApi,
            ImageSource, LoadedImageSource,
            FontSource, LoadedFontSource,
        },
        dom::DomId,
        display_list::SolvedLayout,
        callbacks::PipelineId,
        gl::VirtualGlDriver,
        ui_state::UiState,
        ui_description::UiDescription,
        window::{FullWindowState, LogicalSize, WindowSize},
    };

    fn load_font(_: &FontSource) -> Option<LoadedFontSource> { None }
    fn load_image(_: &ImageSource) -> Option<LoadedImageSource> { None }

    let mut app_resources = AppResources::new();
    let mut render_api = FakeRenderApi::new();

    let fake_window_state = FullWindowState {
        size: WindowSize {
            dimensions: LogicalSize::new(size.0, size.1),
            hidpi_factor: 1.0,
            winit_hidpi_factor: 1.0,
            .. Default::default()
        },
        .. Default::default()
    };
    let gl_context = Rc::new(VirtualGlDriver::new());
    let pipeline_id = PipelineId::new();
    let epoch = Epoch(0);

    // Important!
    app_resources.add_pipeline(pipeline_id);

    DomId::reset();

    let mut ui_state = UiState::new(dom, None);
    let ui_description = UiDescription::new(&mut ui_state, &css, &None, &BTreeMap::new(), false);

    let mut ui_states = BTreeMap::new();
    ui_states.insert(DomId::ROOT_ID, ui_state);
    let mut ui_descriptions = BTreeMap::new();
    ui_descriptions.insert(DomId::ROOT_ID, ui_description);

    // Solve the layout (the extra parameters are necessary because of IFrame recursion)
    let solved_layout = SolvedLayout::new(
        epoch,
        pipeline_id,
        &fake_window_state,
        gl_context,
        &mut render_api,
        &mut app_resources,
        &mut ui_states,
        &mut ui_descriptions,
        azul_core::gl::insert_into_active_gl_textures,
        azul_layout::ui_solver::do_the_layout,
        load_font,
        load_image,
    );

    CachedDisplayList::new(
        epoch,
        pipeline_id,
        &fake_window_state,
        &ui_states,
        &solved_layout.solved_layout_cache,
        &solved_layout.gl_texture_cache,
        &app_resources,
    )
}

fn main() {

    use std::process::exit;

    const TESTS_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

    let mut layout_tests_ok = Vec::new();
    let mut layout_tests_err = Vec::new();

    for (filename, file_contents) in load_files(TESTS_DIRECTORY) {
        let file_xml = match xml::parse_xml_string(&file_contents) {
            Ok(o) => o,
            Err(e) => panic!("File {:?} is not valid XML: error: {:?}, contents: {:?}", filename, e, file_contents),
        };

        // Load all <test> nodes
        for test_xml_node in file_xml.iter().filter(|node| node.node_type == "test") {

            let test_name = find_attribute(&test_xml_node, "name").unwrap();

            let html_node = find_root_node(&test_xml_node.children, "html").unwrap();
            let body_node = find_root_node(&html_node.children, "body").unwrap();
            let style_node = find_root_node(&html_node.children, "style").unwrap();

            let dom = render_dom_from_app_node_inner(&body_node, &XmlComponentMap::default(), &FilteredComponentArguments::default()).unwrap();
            let css = azul_css_parser::new_from_str(get_content(style_node)).unwrap();

            // One <test> can have multiple <output> to test for different sizes
            for expected_output in test_xml_node.children.iter().filter(|node| node.node_type == "output") {
                let expected_output_test = get_content(expected_output)
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .collect::<Vec<&str>>();

                let trim_until = expected_output_test[0].chars().take_while(|c| c.is_whitespace()).count();
                let expected_output_test = expected_output_test.iter().map(|l| &l[trim_until..]).collect::<Vec<&str>>();
                let expected_output_test = expected_output_test.join("\r\n");
                let output_size = parse_size(find_attribute(&expected_output, "size").unwrap()).unwrap();
                let display_list = create_display_list(dom.clone(), &css, output_size);

                let output = format!("{:#?}", display_list.root);
                let output = output.lines().collect::<Vec<&str>>().join("\r\n");

                if output != expected_output_test {
                    let output = output.lines().map(|l| format!("    {}", l)).collect::<Vec<String>>().join("\r\n");
                    let expected_output_test = expected_output_test.lines().map(|l| format!("    {}", l)).collect::<Vec<String>>().join("\r\n");
                    layout_tests_err.push(format!(
                        "layout_test {}:{} at size: {:?} ... FAILED\r\n\r\n    expected:\r\n\r\n{}\r\n\r\n    got:\r\n\r\n{}\r\n\r\n",
                        filename, test_name, output_size, expected_output_test, output
                    ));
                } else {
                    layout_tests_ok.push(format!("layout_test {}:{} @ {:?} ... ok", filename, test_name, output_size));
                }
            }
        }
    }

    if layout_tests_err.is_empty() {
        println!("{}", layout_tests_ok.join("\r\n"));
        println!("\r\nlayout_test result: ok. {} passed; {} failed; 0 ignored; 0 measured; 0 filtered out", layout_tests_ok.len(), layout_tests_err.len());
    } else {
        println!("{}", layout_tests_ok.join("\r\n"));
        println!("{}", layout_tests_err.join("\r\n"));
        eprintln!("\r\nlayout_test result: FAILED. {} passed; {} failed; 0 ignored; 0 measured; 0 filtered out", layout_tests_ok.len(), layout_tests_err.len());
        exit(-1);
    }
}

