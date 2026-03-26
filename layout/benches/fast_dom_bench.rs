/// Benchmark comparing Slow (tree-based) vs Fast (arena-based) DOM construction
/// from XHTML parsing.
///
/// Run with: cargo bench -p azul-layout --bench fast_dom_bench

use std::time::Instant;

fn main() {
    // Use chapter-8.xht as the benchmark file (24k+ lines)
    let bench_file = std::path::Path::new("../doc/xhtml1/chapter-8.xht");
    let xml_content = match std::fs::read_to_string(bench_file) {
        Ok(c) => c,
        Err(e) => {
            // Try from workspace root
            let alt = std::path::Path::new("doc/xhtml1/chapter-8.xht");
            match std::fs::read_to_string(alt) {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("Cannot find benchmark file: {}", e);
                    eprintln!("Run from workspace root or layout/ directory");
                    return;
                }
            }
        }
    };

    println!("Benchmark file: chapter-8.xht ({} bytes, {} lines)",
        xml_content.len(),
        xml_content.lines().count());

    // === Stage breakdown ===
    println!("\n=== Stage breakdown ===");
    {
        // XML tokenize + parse (shared, one-time cost)
        let t0 = Instant::now();
        let parsed = azul_layout::xml::parse_xml_string(&xml_content).unwrap();
        let xml_parse_ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("  XML parse:       {:.2}ms ({} top-level nodes)", xml_parse_ms, parsed.len());

        // get_html_node + get_body_node + CSS extraction
        let t1 = Instant::now();
        let html_node = azul_core::xml::get_html_node(parsed.as_ref()).unwrap();
        let body_node = azul_core::xml::get_body_node(html_node.children.as_ref()).unwrap();
        let mut global_css = None;
        if let Some(head_node) = azul_core::xml::find_node_by_type(html_node.children.as_ref(), "head") {
            if let Some(style_node) = azul_core::xml::find_node_by_type(head_node.children.as_ref(), "style") {
                let text = style_node.get_text_content();
                if !text.is_empty() {
                    global_css = Some(azul_css::css::Css::from_string(text.into()));
                }
            }
        }
        let prep_ms = t1.elapsed().as_secs_f64() * 1000.0;
        println!("  CSS extraction:  {:.2}ms", prep_ms);

        // Fast path: CompactDomBuilder
        let t2 = Instant::now();
        let _fast_dom = azul_core::xml::render_dom_from_body_node_fast(
            &body_node, global_css.clone(), &azul_core::xml::ComponentMap::with_builtin(), None,
        ).unwrap();
        let fast_build_ms = t2.elapsed().as_secs_f64() * 1000.0;
        println!("  FastDom build:   {:.2}ms (CompactDomBuilder → StyledDom)", fast_build_ms);

        // Slow path: tree Dom → CompactDom
        let t3 = Instant::now();
        let _slow_dom = azul_core::xml::render_dom_from_body_node(
            &body_node, global_css, &azul_core::xml::ComponentMap::with_builtin(), None,
        ).unwrap();
        let slow_build_ms = t3.elapsed().as_secs_f64() * 1000.0;
        println!("  SlowDom build:   {:.2}ms (tree Dom → CompactDom → StyledDom)", slow_build_ms);

        // Direct path: XML tokens → FastDom (no XmlNode tree, no Dom tree)
        let t4 = Instant::now();
        let fast_dom_direct = azul_layout::xml::parse_xml_to_fast_dom(&xml_content).unwrap();
        let direct_parse_ms = t4.elapsed().as_secs_f64() * 1000.0;
        println!("  Direct parse:    {:.2}ms (XML tokens → FastDom, {} nodes)",
            direct_parse_ms, fast_dom_direct.node_data.as_ref().len());

        // Direct + StyledDom (single function, includes CSS extraction)
        let t5 = Instant::now();
        let _styled = azul_layout::xml::parse_xml_to_styled_dom(&xml_content).unwrap();
        let direct_styled_ms = t5.elapsed().as_secs_f64() * 1000.0;
        println!("  Direct+styled:   {:.2}ms (XML → StyledDom, single function)", direct_styled_ms);
    }

    let component_map = azul_core::xml::ComponentMap::with_builtin();

    // Parse XML once (shared between both paths)
    let parsed = match azul_layout::xml::parse_xml_string(&xml_content) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("XML parse error: {}", e);
            return;
        }
    };

    println!("Parsed XML: {} top-level nodes", parsed.len());

    const ITERATIONS: usize = 5;

    // === Benchmark SLOW path ===
    println!("\n=== SLOW path (tree Dom → CompactDom → StyledDom) ===");
    let mut slow_times = Vec::new();
    for i in 0..ITERATIONS {
        let t0 = Instant::now();
        let result = azul_core::xml::str_to_dom(parsed.as_ref(), &component_map, None);
        let elapsed = t0.elapsed();
        match result {
            Ok(styled_dom) => {
                let node_count = styled_dom.node_hierarchy.as_ref().len();
                slow_times.push(elapsed);
                println!("  [{}/{}] {} nodes in {:.2}ms",
                    i + 1, ITERATIONS, node_count,
                    elapsed.as_secs_f64() * 1000.0);
            }
            Err(e) => {
                eprintln!("  SLOW path error: {}", e);
                return;
            }
        }
    }

    // === Benchmark FAST path ===
    println!("\n=== FAST path (FastDom → StyledDom, no tree intermediary) ===");
    let mut fast_times = Vec::new();
    for i in 0..ITERATIONS {
        let t0 = Instant::now();
        let result = azul_core::xml::str_to_dom_fast(parsed.as_ref(), &component_map, None);
        let elapsed = t0.elapsed();
        match result {
            Ok(styled_dom) => {
                let node_count = styled_dom.node_hierarchy.as_ref().len();
                fast_times.push(elapsed);
                println!("  [{}/{}] {} nodes in {:.2}ms",
                    i + 1, ITERATIONS, node_count,
                    elapsed.as_secs_f64() * 1000.0);
            }
            Err(e) => {
                eprintln!("  FAST path error: {}", e);
                return;
            }
        }
    }

    // === Summary ===
    if !slow_times.is_empty() && !fast_times.is_empty() {
        let slow_avg = slow_times.iter().map(|t| t.as_secs_f64()).sum::<f64>() / slow_times.len() as f64;
        let fast_avg = fast_times.iter().map(|t| t.as_secs_f64()).sum::<f64>() / fast_times.len() as f64;
        let speedup = slow_avg / fast_avg;

        println!("\n=== RESULTS ===");
        println!("  SLOW avg: {:.2}ms", slow_avg * 1000.0);
        println!("  FAST avg: {:.2}ms", fast_avg * 1000.0);
        println!("  Speedup:  {:.2}x", speedup);
        println!("  Saved:    {:.2}ms per parse ({:.1}%)",
            (slow_avg - fast_avg) * 1000.0,
            (1.0 - fast_avg / slow_avg) * 100.0);
    }
}
