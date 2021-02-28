extern crate azulc;
extern crate azul_core;

use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;

use azul_core::window::LogicalSize;
use azul_core::styled_dom::StyledDom;
use azulc::xml_parser::XmlComponentMap;

#[derive(PartialEq)]
enum Action {
    PrintHelp,
    PrintHtmlCode,
    PrintCCode, // unimplemented, does nothing
    PrintCppCode, // unimplemented, does nothing
    PrintRustCode, // unimplemented, does nothing
    PrintPythonCode, // unimplemented, does nothing
    PrintDisplayList(LogicalSize),
    DisplayFile, // unimplemented, does nothing
}

fn print_help() {
    eprintln!("usage: azulc [OPTIONS] file.xml");
    eprintln!("[OPTIONS]:");
    eprintln!("--language=[rust | c | html]: compile XML file to Rust or C source code");
    eprintln!("--display-list widthxheight");
    eprintln!("");
    eprintln!("If OPTIONS is empty, the file will be displayed in a window.");
}

fn main() {

    let args = env::args().collect::<Vec<String>>();

    let input_file = args.last();
    let second_arg = args.get(2);

    // select action
    let action = match second_arg.as_ref().map(|s| s.as_str()) {
        Some("--help")                  => Action::PrintHelp,
        Some("--language=rust")         => Action::PrintRustCode,
        Some("--language=html")         => Action::PrintHtmlCode,
        Some("--language=c")            => Action::PrintCCode,
        Some("--language=cpp")          => Action::PrintCppCode,
        Some("--language=python")       => Action::PrintPythonCode,
        Some("--display-list")          => {
            let size = env::args().nth(3).expect("no output size specified for display list");
            let size_parsed = match azulc::parse_display_list_size(&size) {
                Some(s) => s,
                None => {
                    eprintln!("error: display list size \"{}\" could not be parsed", size);
                    print_help();
                    exit(-1);
                }
            };
            Action::PrintDisplayList(LogicalSize::new(size_parsed.0, size_parsed.1))
        },
        Some(other) => {
            eprintln!("unknown command: \"{}\"", other);
            print_help();
            exit(-1);
        }
        None => Action::DisplayFile,
    };

    process(action, input_file)
}

fn process(action: Action, file: Option<&String>) {

    use azulc::xml_parser::*;

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

    let styled_dom = match str_to_dom(&root_nodes, &mut XmlComponentMap::default()) {
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
        Action::PrintHtmlCode => {
            println!("{}", styled_dom.get_html_string());
        },
        Action::PrintCCode => {
            println!("{}", get_c_code(&styled_dom, &mut XmlComponentMap::default()));
        },
        Action::PrintCppCode => {
            println!("{}", get_cpp_code(&styled_dom, &mut XmlComponentMap::default()));
        },
        Action::PrintRustCode => {
            println!("{}", get_rust_code(&styled_dom, &mut XmlComponentMap::default()));
        },
        Action::PrintPythonCode => {
            println!("{}", get_python_code(&styled_dom, &mut XmlComponentMap::default()));
        },
        Action::PrintDisplayList(size) => {

            use azul_core::{
                app_resources::{AppResources, IdNamespace, LoadImageFn, LoadFontFn, Epoch},
                display_list::SolvedLayout,
                callbacks::PipelineId,
                gl::OptionGlContextPtr,
                window::{WindowSize, FullWindowState},
                display_list::{CachedDisplayList, RenderCallbacks},
            };

            // Set width + height of the rendering here
            let mut fake_window_state = FullWindowState::default();
            fake_window_state.size.dimensions = size;

            let mut app_resources = AppResources::new();
            let gl_context = OptionGlContextPtr::None;
            let pipeline_id = PipelineId::new();
            let epoch = Epoch(0);

            let fc_cache = azulc::font_loading::build_font_cache();

            // Important!
            app_resources.add_pipeline(pipeline_id);

            let mut resource_updates = Vec::new();
            let callbacks = RenderCallbacks {
                insert_into_active_gl_textures: azul_core::gl::insert_into_active_gl_textures,
                layout_fn: azul_layout::do_the_layout,
                load_font_fn: LoadFontFn { cb: azulc::font_loading::font_source_get_bytes }, // needs feature="font_loading"
                load_image_fn: LoadImageFn { cb: azulc::image_loading::image_source_get_bytes }, // needs feature="image_loading"
                parse_font_fn: azul_layout::text_layout::parse_font_fn, // needs feature="text_layout"
            };

            // Solve the layout (the extra parameters are necessary because of IFrame recursion)
            let solved_layout = SolvedLayout::new(
                styled_dom,
                epoch,
                pipeline_id,
                &fake_window_state,
                &gl_context,
                &mut resource_updates,
                IdNamespace(0),
                &mut app_resources,
                callbacks,
                &fc_cache,
            );

            let display_list = CachedDisplayList::new(
                epoch,
                pipeline_id,
                &fake_window_state,
                &solved_layout.layout_results,
                &solved_layout.gl_texture_cache,
                &app_resources,
            );

            println!("{:#?}", display_list.root);
        },
        Action::DisplayFile => {
            // TODO: open window and show the file
        },
        // Action::RenderToPng(output_path) -- TODO!
    }
}

fn get_rust_code(dom: &StyledDom, components: &mut XmlComponentMap) -> String {
    String::new() // TODO
}

fn get_c_code(dom: &StyledDom, components: &mut XmlComponentMap) -> String {
    String::new() // TODO
}

fn get_cpp_code(dom: &StyledDom, components: &mut XmlComponentMap) -> String {
    String::new() // TODO
}

fn get_python_code(dom: &StyledDom, components: &mut XmlComponentMap) -> String {
    String::new() // TODO
}