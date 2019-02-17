extern crate azul;

use azul::xml::{parse_xml_string, expand_xml_components};

const TEST_XML: &str = include_str!("./ui.xml");

fn main() {
    let parsed_xml = parse_xml_string(TEST_XML).unwrap();
    let expanded_xml = expand_xml_components(&parsed_xml).unwrap();
    println!("expanded_xml: {:#?}", expanded_xml);
}