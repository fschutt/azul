extern crate azulc_lib;
extern crate azul_core;

use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;

use azul_core::window::LogicalSize;
use azul_core::styled_dom::StyledDom;
use azul_core::app_resources::{Epoch, AppResources};
use azul_core::callbacks::PipelineId;
use azul_core::ui_solver::LayoutResult;
use azul_core::display_list::GlTextureCache;
use azul_core::{
    app_resources::{IdNamespace, LoadImageFn, LoadFontFn},
    display_list::SolvedLayout,
    gl::OptionGlContextPtr,
    window::FullWindowState,
    display_list::{CachedDisplayList, RenderCallbacks},
};

use azulc_lib::xml_parser::XmlComponentMap;
use azulc_lib::xml_parser::XmlNode;

#[derive(PartialEq)]
enum Action {
    PrintHelp,
    PrintHtmlCode,
    PrintStyledDom,
    PrintRustCode,
    PrintCCode, // unimplemented, does nothing
    PrintCppCode, // unimplemented, does nothing
    PrintPythonCode, // unimplemented, does nothing
    PrintDebugLayout(LogicalSize),
    PrintScrollClips(LogicalSize),
    PrintDisplayList(LogicalSize),
}

fn print_help() {
    eprintln!("usage: azulc [OPTIONS] file.xml");
    eprintln!("");
    eprintln!("[OPTIONS]:");
    eprintln!("    --language=[rust | c | python | cpp | html]: compile XML file to source code");
    eprintln!("    --debug-layout WIDTHxHEIGHT: print a debug output of the layout solver");
    eprintln!("    --display-list WIDTHxHEIGHT: print the display list given WIDTH and HEIGHT");
    eprintln!("    --scroll-clips WIDTHxHEIGHT: print the overflowing scroll clips given WIDTH and HEIGHT");
    eprintln!("    --cascade: print the cascaded styled DOM");
    eprintln!("");
    eprintln!("If OPTIONS is empty, the file will be printed to Rust code");
}

fn main() {

    let args = env::args().collect::<Vec<String>>();

    if args.len() == 1 {
        // no input file
        eprintln!("error: no input file given");
        eprintln!("");
        print_help();
        return;
    }

    let input_file = args.last();

    // select action
    let second_arg = args.get(1);
    let action = match second_arg.as_ref().map(|s| s.as_str()) {
        Some("--help")                  => Action::PrintHelp,
        Some("--cascade")               => Action::PrintStyledDom,
        Some("--language=rust")         => Action::PrintRustCode,
        Some("--language=html")         => Action::PrintHtmlCode,
        Some("--language=c")            => Action::PrintCCode,
        Some("--language=cpp")          => Action::PrintCppCode,
        Some("--language=python")       => Action::PrintPythonCode,
        Some("--debug-layout")          => {
            let size = env::args().nth(2).expect("no output size specified for display list");
            let size_parsed = match azulc_lib::parse_display_list_size(&size) {
                Some(s) => s,
                None => {
                    eprintln!("error: debug layout size \"{}\" could not be parsed", size);
                    print_help();
                    exit(-1);
                }
            };
            Action::PrintDebugLayout(LogicalSize::new(size_parsed.0, size_parsed.1))
        },
        Some("--scroll-clips")          => {
            let size = env::args().nth(2).expect("no output size specified for display list");
            let size_parsed = match azulc_lib::parse_display_list_size(&size) {
                Some(s) => s,
                None => {
                    eprintln!("error: scroll clip size \"{}\" could not be parsed", size);
                    print_help();
                    exit(-1);
                }
            };
            Action::PrintScrollClips(LogicalSize::new(size_parsed.0, size_parsed.1))
        },
        Some("--display-list")          => {
            let size = env::args().nth(2).expect("no output size specified for display list");
            let size_parsed = match azulc_lib::parse_display_list_size(&size) {
                Some(s) => s,
                None => {
                    eprintln!("error: display list size \"{}\" could not be parsed", size);
                    print_help();
                    exit(-1);
                }
            };
            Action::PrintDisplayList(LogicalSize::new(size_parsed.0, size_parsed.1))
        },
        _ => Action::PrintRustCode,
    };

    process(action, input_file)
}

fn process(action: Action, file: Option<&String>) {

    use azulc_lib::xml_parser::*;

    if action == Action::PrintHelp {
        print_help();
        exit(0);
    }

    let input_file = match file {
       Some(s) => s,
       None => {
           eprintln!("error: no input file given");
           print_help();
           exit(-1);
       },
   };

    let file_contents = match fs::read_to_string(input_file.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file: \"{}\" - error:\r\n{}", input_file, e);
            exit(-1);
        },
    };

    // parse the XML
    let root_nodes = match parse_xml_string(&file_contents).ok() {
        Some(s) => s,
        None => {
            eprintln!("error: input could not be parsed as xml");
            print_help();
            exit(-1);
        }
    };

    let styled_dom = match str_to_dom(root_nodes.as_ref(), &mut XmlComponentMap::default()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not render DOM:\r\n{}", e);
            print_help();
            exit(-1);
        }
    };

    match action {
        Action::PrintHelp => {
            print_help();
            exit(0);
        },
        Action::PrintStyledDom => {
            println!("{:#?}", styled_dom);
        },
        Action::PrintHtmlCode => {
            println!("{}", styled_dom.get_html_string("", ""));
        },
        Action::PrintRustCode => {
            match get_rust_code(root_nodes.as_ref()) {
                Ok(o) => { println!("{}", o); },
                Err(e) => { eprintln!("{}", e); },
            }
        },
        Action::PrintCCode => {
            match get_c_code(root_nodes.as_ref()) {
                Ok(o) => { println!("{}", o); },
                Err(e) => { eprintln!("{}", e); },
            }
        },
        Action::PrintCppCode => {
            match get_cpp_code(root_nodes.as_ref()) {
                Ok(o) => { println!("{}", o); },
                Err(e) => { eprintln!("{}", e); },
            }
        },
        Action::PrintPythonCode => {
            match get_python_code(root_nodes.as_ref()) {
                Ok(o) => { println!("{}", o); },
                Err(e) => { eprintln!("{}", e); },
            }
        },
        Action::PrintDebugLayout(size) => {
            let pipeline_id = PipelineId::new();
            let epoch = Epoch(0);
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;
            let mut app_resources = AppResources::new();
            let layout = solve_layout(styled_dom, size, pipeline_id, epoch, &fake_window_state, &mut app_resources);
            let layout_debug = layout_result_print_layout(&layout);
            println!("{}", layout_debug);
        },
        Action::PrintScrollClips(size) => {
            let pipeline_id = PipelineId::new();
            let epoch = Epoch(0);
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;
            let mut app_resources = AppResources::new();
            let layout = solve_layout(styled_dom, size, pipeline_id, epoch, &fake_window_state, &mut app_resources);
            println!("{:#?}", layout.scrollable_nodes);
        },
        Action::PrintDisplayList(size) => {
            let pipeline_id = PipelineId::new();
            let epoch = Epoch(0);
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;
            let mut app_resources = AppResources::new();
            let layout = solve_layout(styled_dom, size, pipeline_id, epoch, &fake_window_state, &mut app_resources);
            let display_list = CachedDisplayList::new(
                epoch,
                pipeline_id,
                &fake_window_state,
                &[layout],
                &GlTextureCache::default(),
                &app_resources,
            );

            println!("{:#?}", display_list.root);
        },
        // Action::DisplayFile => // TODO: open window and show the file,
        // Action::RenderToPng(output_path) -- TODO!
    }
}

fn solve_layout(
    styled_dom: StyledDom,
    size: LogicalSize,
    pipeline_id: PipelineId,
    epoch: Epoch,
    fake_window_state: &FullWindowState,
    app_resources: &mut AppResources
) -> LayoutResult {

    let gl_context = OptionGlContextPtr::None;

    let fc_cache = azulc_lib::font_loading::build_font_cache();

    // Important!
    app_resources.add_pipeline(pipeline_id);

    let mut resource_updates = Vec::new();
    let callbacks = RenderCallbacks {
        insert_into_active_gl_textures: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::do_the_layout,
        load_font_fn: LoadFontFn { cb: azulc_lib::font_loading::font_source_get_bytes }, // needs feature="font_loading"
        load_image_fn: LoadImageFn { cb: azulc_lib::image_loading::image_source_get_bytes }, // needs feature="image_loading"
        parse_font_fn: azul_layout::text_layout::parse_font_fn, // needs feature="text_layout"
    };

    // Solve the layout (the extra parameters are necessary because of IFrame recursion)
    let mut solved_layout = SolvedLayout::new(
        styled_dom,
        epoch,
        pipeline_id,
        &fake_window_state,
        &gl_context,
        &mut resource_updates,
        IdNamespace(0),
        app_resources,
        callbacks,
        &fc_cache,
    );

    solved_layout.layout_results.remove(0)
}

fn layout_result_print_layout(result: &LayoutResult) -> String {

    use azul_core::styled_dom::ParentWithNodeDepth;

    let mut s = String::new();

    for ParentWithNodeDepth { depth, node_id } in result.styled_dom.non_leaf_nodes.as_ref().iter() {

        let parent_node_id = match node_id.into_crate_internal() { Some(s) => s, None => continue, };
        let tabs = "    ".repeat(*depth);
        let width = result.width_calculated_rects.as_ref()[parent_node_id];
        let height = result.height_calculated_rects.as_ref()[parent_node_id];
        let x_pos = result.solved_pos_x.as_ref()[parent_node_id].0;
        let y_pos = result.solved_pos_y.as_ref()[parent_node_id].0;

        s.push_str(&format!("{}parent {}: {}x{} @ ({}, {}) (intrinsic={}x{}, flex_grow={}x{})\r\n",
                 tabs, parent_node_id, width.total(), height.total(), x_pos, y_pos,
                 width.min_inner_size_px,height.min_inner_size_px, width.flex_grow_px, height.flex_grow_px
        ));

        for child_id in parent_node_id.az_children(&result.styled_dom.node_hierarchy.as_container()) {

            let tabs = "    ".repeat(*depth + 1);
            let width = result.width_calculated_rects.as_ref()[child_id];
            let height = result.height_calculated_rects.as_ref()[child_id];
            let x_pos = result.solved_pos_x.as_ref()[child_id].0;
            let y_pos = result.solved_pos_y.as_ref()[child_id].0;

            s.push_str(&format!("{}child {}: {}x{} @ ({}, {}) (intrinsic={}x{}, flex_grow={}x{})\r\n",
                     tabs, child_id, width.total(), height.total(), x_pos, y_pos,
                     width.min_inner_size_px,height.min_inner_size_px, width.flex_grow_px, height.flex_grow_px
            ));
        }
    }

    s
}

fn get_rust_code(root_nodes: &[XmlNode]) -> Result<String, String> {
    azulc_lib::xml_parser::str_to_rust_code(root_nodes, "", &mut XmlComponentMap::default()).map_err(|e| format!("{}", e))
}

fn get_c_code(root_nodes: &[XmlNode]) -> Result<String, String> {
    Ok(String::new()) // TODO
}

fn get_cpp_code(root_nodes: &[XmlNode]) -> Result<String, String> {
    Ok(String::new()) // TODO
}

fn get_python_code(root_nodes: &[XmlNode]) -> Result<String, String> {
    Ok(String::new()) // TODO
}