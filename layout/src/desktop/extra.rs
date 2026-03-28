use azul_css::props::basic::color::ColorU;

pub fn coloru_from_str(s: &str) -> ColorU {
    azul_css::props::basic::color::parse_css_color(s)
        .ok()
        .unwrap_or(ColorU::BLACK)
}
