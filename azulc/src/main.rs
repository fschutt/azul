extern crate azulc_lib;
extern crate azul_core;

use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;

use azul_core::{
    gl::OptionGlContextPtr,
    window::FullWindowState,
    xml::{XmlComponentMap, XmlNode},
    window::LogicalSize,
    styled_dom::{StyledDom, DomId},
    callbacks::{PipelineId, DocumentId},
    ui_solver::LayoutResult,
    app_resources::{
        IdNamespace, LoadFontFn,
        Epoch, RendererResources,
        ImageCache, GlTextureCache,
    },
    display_list::{
        SolvedLayout,
        CachedDisplayList,
        RenderCallbacks
    },
};

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

    use azul_core::xml::*;
    use azulc_lib::xml::parse_xml_string;

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
            println!("{}", styled_dom.get_html_string("", "", false));
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
            let epoch = Epoch::new();
            let document_id = DocumentId {
                namespace_id: IdNamespace(0),
                id: 0,
            };
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;
            let mut renderer_resources = RendererResources::default();
            let layout = solve_layout(styled_dom, size, document_id, epoch, &fake_window_state, &mut renderer_resources);
            let layout_debug = layout_result_print_layout(&layout);
            println!("{}", layout_debug);
        },
        Action::PrintScrollClips(size) => {
            let document_id = DocumentId {
                namespace_id: IdNamespace(0),
                id: 0,
            };
            let epoch = Epoch::new();
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;
            let mut renderer_resources = RendererResources::default();
            let layout = solve_layout(styled_dom, size, document_id, epoch, &fake_window_state, &mut renderer_resources);
            println!("{:#?}", layout.scrollable_nodes);
        },
        Action::PrintDisplayList(size) => {
            let epoch = Epoch::new();
            let document_id = DocumentId {
                namespace_id: IdNamespace(0),
                id: 0,
            };
            let dom_id = DomId { inner: 0 };
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;
            let mut renderer_resources = RendererResources::default();
            let image_cache = ImageCache::default();
            let layout = solve_layout(styled_dom, size, document_id, epoch, &fake_window_state, &mut renderer_resources);
            let display_list = LayoutResult::get_cached_display_list(
                &document_id,
                dom_id,
                epoch,
                &[layout],
                &fake_window_state,
                &GlTextureCache::default(),
                &renderer_resources,
                &image_cache,
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
    document_id: DocumentId,
    epoch: Epoch,
    fake_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources
) -> LayoutResult {

    let fc_cache = azulc_lib::font_loading::build_font_cache();
    let image_cache = ImageCache::default();
    let callbacks = RenderCallbacks {
        insert_into_active_gl_textures_fn: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::do_the_layout,
        load_font_fn: azulc_lib::font_loading::font_source_get_bytes, // needs feature="font_loading"
        parse_font_fn: azul_layout::parse_font_fn, // needs feature="text_layout"
    };

    // Solve the layout (the extra parameters are necessary because of IFrame recursion)
    let mut resource_updates = Vec::new();
    let mut solved_layout = SolvedLayout::new(
        styled_dom,
        epoch,
        &document_id,
        &fake_window_state,
        &mut resource_updates,
        IdNamespace(0),
        &image_cache,
        &fc_cache,
        &callbacks,
        renderer_resources,
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
    azul_core::xml::str_to_rust_code(root_nodes, "", &mut XmlComponentMap::default()).map_err(|e| format!("{}", e))
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