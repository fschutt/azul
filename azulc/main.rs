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
        let root_nodes = match parse_xml_string(&file_contents).ok() {
            Some(s) => s,
            None => {
                eprintln!("error: input could not be parsed as xml");
                print_help();
                exit(-1);
            }
        };

        use std::path::Path;

        let input_file_path = Path::new(&input_file);
        let base = input_file_path.parent().and_then(|p| p.to_str()).unwrap_or("");

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
                let dom = match str_to_dom(&root_nodes, &mut XmlComponentMap::<Dummy>::default()) {
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
                use azul_css::get_css_key_map;

                let dom = match str_to_dom(&root_nodes, &mut XmlComponentMap::<Dummy>::default()) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not render DOM:\r\n{}", e);
                        print_help();
                        exit(-1);
                    }
                };

                // load the CSS file from the head -> link href="css" node
                let css = match load_style_file_from_xml(base, &root_nodes) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not find CSS file:\r\n{:?}", e);
                        print_help();
                        exit(-1);
                    }
                };

                let ui_description = cascade_dom(dom, &css);
                
                let css_key_map = get_css_key_map();

                for (node_id, styled_node) in ui_description.styled_nodes.internal.iter().enumerate() {
                    println!("node {}:", node_id);
                    for (css_key, css_value) in &styled_node.css_constraints {
                        println!("\t{}: {},", css_key.to_str(&css_key_map), css_value.to_str());
                    }
                }
            },
            Action::PrintDisplayList((w, h)) => {
                use azul_core::window::LogicalSize;                                            

                let dom = match str_to_dom(&root_nodes, &mut XmlComponentMap::<Dummy>::default()) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not render DOM:\r\n{}", e);
                        print_help();
                        exit(-1);
                    }
                };

                let css = match load_style_file_from_xml(base, &root_nodes) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not find CSS file:\r\n{:?}", e);
                        print_help();
                        exit(-1);
                    }
                };
                
                let cached_display_list = layout_dom( // "not found in scope error": you forgot to compile azulc with "--all-features" !
                    dom, &css, LogicalSize::new(w, h)
                );
                println!("{:#?}", cached_display_list.root);
            },
        }
    } else if input_file.ends_with(".css") {
        // compile CSS file to Rust code
        let css = match azul_css_parser::new_from_str(&file_contents) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: could not parse CSS:\r\n{}", e);
                print_help();
                exit(-1);
            }
        };
        println!("{}", azulc::css::css_to_rust_code(&css));
    } else if input_file == "--help" {
        print_help();
    } else {
        panic!("invalid input file type, can only process \".html\", \".xml\" or \".css\"");
    }
}