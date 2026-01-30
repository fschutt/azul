//! XML parsing performance test
//! 
//! Run with: cargo run --release --example xml_perf_test

use std::time::Instant;

fn main() {
    // Read the test HTML file
    let html_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: xml_perf_test <path-to-html>");
        eprintln!("Example: cargo run --release --example xml_perf_test /path/to/printpdf.html");
        std::process::exit(1);
    });
    
    println!("Loading HTML from: {}", html_path);
    let html = std::fs::read_to_string(&html_path).expect("Failed to read HTML file");
    println!("HTML size: {} bytes ({} KB)", html.len(), html.len() / 1024);
    
    // Count some statistics
    let span_count = html.matches("<span").count();
    let div_count = html.matches("<div").count();
    let total_tags = html.matches("<").count();
    println!("Approximate tag counts: {} spans, {} divs, {} total '<' chars", span_count, div_count, total_tags);
    
    println!("\n=== Starting XML parsing benchmark ===\n");
    
    let start = Instant::now();
    
    // Call the XML parser
    match azul_layout::xml::parse_xml_string(&html) {
        Ok(nodes) => {
            let elapsed = start.elapsed();
            println!("\n=== Parsing complete ===");
            println!("Time: {:?}", elapsed);
            println!("Root nodes: {}", nodes.len());
            
            // Count total nodes recursively
            fn count_nodes(nodes: &[azul_core::xml::XmlNodeChild]) -> usize {
                let mut count = nodes.len();
                for node in nodes {
                    if let azul_core::xml::XmlNodeChild::Element(elem) = node {
                        count += count_nodes(elem.children.as_ref());
                    }
                }
                count
            }
            
            let total_nodes = count_nodes(&nodes);
            println!("Total nodes (recursive): {}", total_nodes);
            println!("Nodes per second: {:.0}", total_nodes as f64 / elapsed.as_secs_f64());
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("\n=== Parsing FAILED after {:?} ===", elapsed);
            println!("Error: {:?}", e);
        }
    }
}
