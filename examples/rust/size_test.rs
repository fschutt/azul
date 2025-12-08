use azul_css::props::layout::grid::{GridLine, NamedGridLine};
use azul_css::corety::{AzString, U8Vec};
use std::mem::size_of;

fn main() {
    println!("U8Vec: {} bytes ({} bits)", size_of::<U8Vec>(), size_of::<U8Vec>() * 8);
    println!("AzString: {} bytes ({} bits)", size_of::<AzString>(), size_of::<AzString>() * 8);
    println!("NamedGridLine: {} bytes ({} bits)", size_of::<NamedGridLine>(), size_of::<NamedGridLine>() * 8);
    println!("GridLine: {} bytes ({} bits)", size_of::<GridLine>(), size_of::<GridLine>() * 8);
}
