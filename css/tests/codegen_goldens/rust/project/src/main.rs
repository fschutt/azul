mod styles;

fn main() {
    let style_btn = styles::style_btn();
    println!("style_btn: {} properties", style_btn.len());
}
