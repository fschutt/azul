extern crate azulc;
extern crate azul_core;
extern crate azul_css_parser;

use std::env;
use std::fs;

enum Action {
    PrintRustCode,
    Cascade,
    PrintDom,
    PrintDisplayList((f32, f32)),
}

#[no_mangle]
pub fn compile_css_to_rust_code(_input: &str) -> String {
    String::new()
}

fn print_help() {
    eprintln!("usage: azulc [file.xml | .html] [--rust | --cascade | --dom | --display-list widthxheight]");
    eprintln!("usage: azulc file.css");
}

fn main() {
    
    use azulc::xml::*;
    use std::process::exit;

    struct Dummy;

    let input_file = match env::args().nth(1) {
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

    if input_file.ends_with(".html") || input_file.ends_with(".xml") {
        
        // process XML / HTML
        
        let second_arg = env::args().nth(2).unwrap_or(String::from("--rust"));

        let action = match second_arg.as_str() {
            "--rust"            => Action::PrintRustCode,
            "--cascade"         => Action::Cascade,
            "--dom"             => Action::PrintDom,
            "--display-list"    => {
                let size = env::args().nth(3).expect("no output size specified for display list");
                let size_parsed = match azul_core::display_list::parse_display_list_size(&size) {
                    Some(s) => s,
                    None => {
                        eprintln!("error: size \"{}\" could not be parsed", size);
                        print_help();
                        exit(-1);
                    }
                };
                Action::PrintDisplayList(size_parsed)
            },
            _ => {
                eprintln!("error: invalid second CLI argument");
                print_help();
                exit(-1);
            },
        };

        // parse the XML
        let mut component_map = XmlComponentMap::<Dummy>::default();
        let root_nodes = match parse_xml_string(&file_contents).ok() {
            Some(s) => s,
            None => {
                eprintln!("error: input could not be parsed as xml");
                print_help();
                exit(-1);
            }
        };

        match get_xml_components(&root_nodes, &mut component_map).ok() {
            Some(s) => s,
            None => {
                eprintln!("error: could not parse XML components");
                print_help();
                exit(-1);
            }
        }

        let body_node = match get_body_node(&root_nodes).ok() {
            Some(s) => s,
            None => {
                eprintln!("error: no body / root node");
                print_help();
                exit(-1);
            }
        };

        match action {
            Action::PrintRustCode => {
                let compiled_source = match str_to_rust_code(&root_nodes, "use azul_core::dom::*;", &mut XmlComponentMap::default()) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not render Rust code:\r\n{:?}", e);
                        print_help();
                        exit(-1);
                    }
                };
                println!("{}", compiled_source);
            },
            Action::PrintDom => {
                let dom = match render_dom_from_body_node(&body_node, &component_map) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not render DOM:\r\n{}", e);
                        print_help();
                        exit(-1);
                    }
                };

                println!("{}", dom.get_html_string());
            },
            Action::Cascade => {
                println!("cascading dom + css!");
                // println!("{:#?}", azul_core::style::cascade(&file_contents));
            },
            Action::PrintDisplayList((w, h)) => {
                println!("layouting to display list");
                // println!("{:#?}", azulc::compile_xml_to_display_list(&file_contents, w, h));
            }
        }
    } else if input_file.ends_with(".css") {
        // compile CSS file to Rust code
        let css = azul_css_parser::new_from_str(&file_contents).unwrap();
        println!("{}", azulc::css::css_to_rust_code(&css));
    } else if input_file == "--help" {
        print_help();
    } else {
        panic!("invalid input file type, can only process \".html\", \".xml\" or \".css\"");
    }
}