//! Example: Converting a DisplayList to PDF operations
//!
//! This example demonstrates how to use azul-layout's PDF module to convert
//! a DisplayList into intermediate PDF operations that can be consumed by
//! any PDF library (like printpdf).
//!
//! Run with: cargo run --example pdf_export --features pdf

#[cfg(feature = "pdf")]
fn main() {
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_css::props::basic::ColorU;
    use azul_layout::{
        pdf::{display_list_to_pdf_ops, PdfOp},
        solver3::display_list::{BorderRadius, DisplayList, DisplayListItem},
    };

    // Create a simple display list with a rectangle
    let mut display_list = DisplayList::default();
    display_list.items.push(DisplayListItem::Rect {
        bounds: LogicalRect {
            origin: LogicalPosition::new(50.0, 50.0),
            size: LogicalSize::new(200.0, 100.0),
        },
        color: ColorU {
            r: 100,
            g: 150,
            b: 200,
            a: 255,
        },
        border_radius: BorderRadius::default(),
    });

    // Convert to PDF operations
    let page_size = LogicalSize::new(595.0, 842.0); // A4 in points
    let pdf_page = display_list_to_pdf_ops(&display_list, page_size);

    println!("Generated PDF page with {} operations", pdf_page.ops.len());
    println!(
        "Page size: {}x{} pts",
        pdf_page.page_size.width, pdf_page.page_size.height
    );

    // Print the operations
    for (i, op) in pdf_page.ops.iter().enumerate() {
        println!("Op {}: {:?}", i, op);
    }

    println!("\nResources needed:");
    println!("  Fonts: {}", pdf_page.resources.get_fonts().len());
    println!("  Images: {}", pdf_page.resources.get_images().len());

    println!("\nTo actually generate a PDF file, you would:");
    println!("1. Use the printpdf crate (or similar) in your application");
    println!("2. Iterate through pdf_page.ops");
    println!("3. Translate each PdfOp to the corresponding printpdf call");
    println!("4. Embed fonts and images from pdf_page.resources");
    println!("5. Save the PDF document");
}

#[cfg(not(feature = "pdf"))]
fn main() {
    eprintln!("This example requires the 'pdf' feature.");
    eprintln!("Run with: cargo run --example pdf_export --features pdf");
}
