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
