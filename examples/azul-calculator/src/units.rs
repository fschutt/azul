//! The unit converter's table: eight categories, each with a base unit and
//! its units as exact decimal factors (a mile IS 1609.344 m), temperature
//! with offsets.
//!
//! A unit converts to its category's base as `(v + offset) * num / den` and
//! back as `b * den / num - offset`; `num / den` keeps factors such as 5/9
//! (Fahrenheit) and 1/3.6 (km/h) exact.

use crate::num::{CalcError, Format, Num};

/// One unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    /// `kilometres`.
    pub name: &'static str,
    /// `km`.
    pub symbol: &'static str,
    /// The factor to the base unit, numerator and denominator.
    pub num: &'static str,
    pub den: &'static str,
    /// Added before the factor (temperature).
    pub offset: &'static str,
}

const fn unit(name: &'static str, symbol: &'static str, num: &'static str) -> Unit {
    Unit {
        name,
        symbol,
        num,
        den: "1",
        offset: "0",
    }
}

const fn ratio(name: &'static str, symbol: &'static str, num: &'static str, den: &'static str) -> Unit {
    Unit {
        name,
        symbol,
        num,
        den,
        offset: "0",
    }
}

/// A category of units of one dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Category {
    pub name: &'static str,
    pub units: &'static [Unit],
    /// The units the converter starts with (from, to).
    pub default_pair: (usize, usize),
}

/// The categories, in the order of the drop-down.
pub const CATEGORIES: [Category; 8] = [
    Category {
        name: "Length",
        units: &[
            unit("millimetres", "mm", "0.001"),
            unit("centimetres", "cm", "0.01"),
            unit("metres", "m", "1"),
            unit("kilometres", "km", "1000"),
            unit("inches", "in", "0.0254"),
            unit("feet", "ft", "0.3048"),
            unit("yards", "yd", "0.9144"),
            unit("miles", "mi", "1609.344"),
            unit("nautical miles", "nmi", "1852"),
        ],
        default_pair: (3, 7),
    },
    Category {
        name: "Mass",
        units: &[
            unit("milligrams", "mg", "0.000001"),
            unit("grams", "g", "0.001"),
            unit("kilograms", "kg", "1"),
            unit("tonnes", "t", "1000"),
            unit("ounces", "oz", "0.028349523125"),
            unit("pounds", "lb", "0.45359237"),
            unit("stones", "st", "6.35029318"),
        ],
        default_pair: (2, 5),
    },
    Category {
        name: "Temperature",
        units: &[
            Unit {
                name: "degrees Celsius",
                symbol: "\u{b0}C",
                num: "1",
                den: "1",
                offset: "273.15",
            },
            Unit {
                name: "degrees Fahrenheit",
                symbol: "\u{b0}F",
                num: "5",
                den: "9",
                offset: "459.67",
            },
            unit("kelvin", "K", "1"),
        ],
        default_pair: (1, 0),
    },
    Category {
        name: "Area",
        units: &[
            unit("square millimetres", "mm\u{b2}", "0.000001"),
            unit("square centimetres", "cm\u{b2}", "0.0001"),
            unit("square metres", "m\u{b2}", "1"),
            unit("hectares", "ha", "10000"),
            unit("square kilometres", "km\u{b2}", "1000000"),
            unit("square inches", "in\u{b2}", "0.00064516"),
            unit("square feet", "ft\u{b2}", "0.09290304"),
            unit("square yards", "yd\u{b2}", "0.83612736"),
            unit("acres", "ac", "4046.8564224"),
            unit("square miles", "mi\u{b2}", "2589988.110336"),
        ],
        default_pair: (2, 6),
    },
    Category {
        name: "Volume",
        units: &[
            unit("millilitres", "mL", "0.001"),
            unit("litres", "L", "1"),
            unit("cubic metres", "m\u{b3}", "1000"),
            unit("teaspoons (US)", "tsp", "0.00492892159375"),
            unit("tablespoons (US)", "tbsp", "0.01478676478125"),
            unit("fluid ounces (US)", "fl oz", "0.0295735295625"),
            unit("cups (US)", "cup", "0.2365882365"),
            unit("pints (US)", "pt", "0.473176473"),
            unit("quarts (US)", "qt", "0.946352946"),
            unit("gallons (US)", "gal", "3.785411784"),
            unit("gallons (imperial)", "imp gal", "4.54609"),
        ],
        default_pair: (1, 9),
    },
    Category {
        name: "Speed",
        units: &[
            unit("metres per second", "m/s", "1"),
            ratio("kilometres per hour", "km/h", "1000", "3600"),
            unit("miles per hour", "mph", "0.44704"),
            ratio("knots", "kn", "1852", "3600"),
            unit("feet per second", "ft/s", "0.3048"),
        ],
        default_pair: (1, 2),
    },
    Category {
        name: "Data",
        units: &[
            ratio("bits", "bit", "1", "8"),
            unit("bytes", "B", "1"),
            unit("kilobytes", "kB", "1000"),
            unit("megabytes", "MB", "1000000"),
            unit("gigabytes", "GB", "1000000000"),
            unit("terabytes", "TB", "1000000000000"),
            unit("kibibytes", "KiB", "1024"),
            unit("mebibytes", "MiB", "1048576"),
            unit("gibibytes", "GiB", "1073741824"),
            unit("tebibytes", "TiB", "1099511627776"),
        ],
        default_pair: (4, 8),
    },
    Category {
        name: "Time",
        units: &[
            unit("milliseconds", "ms", "0.001"),
            unit("seconds", "s", "1"),
            unit("minutes", "min", "60"),
            unit("hours", "h", "3600"),
            unit("days", "d", "86400"),
            unit("weeks", "wk", "604800"),
            unit("years (365.2425 d)", "yr", "31556952"),
        ],
        default_pair: (3, 2),
    },
];

/// The category by name (any case).
#[must_use]
pub fn category(name: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

fn num(text: &str) -> Result<Num, CalcError> {
    Num::parse(text)
}

/// `value` in `from` converted to `to` (both of one category).
pub fn convert(value: &Num, from: &Unit, to: &Unit) -> Result<Num, CalcError> {
    let base = value
        .add(&num(from.offset)?)?
        .mul(&num(from.num)?)?
        .div(&num(from.den)?)?;
    base.mul(&num(to.den)?)?.div(&num(to.num)?)?.sub(&num(to.offset)?)
}

/// A converted value as the converter shows it: up to `digits` significant
/// digits, grouped.
#[must_use]
pub fn show(value: &Num, digits: u64) -> String {
    value.format(&Format {
        digits,
        grouping: true,
        scientific: false,
    })
}

/// The rate line under the fields: `1 km = 0.621371 mi`.
pub fn rate_line(from: &Unit, to: &Unit) -> Result<String, CalcError> {
    let one = Num::from_i64(1);
    Ok(format!(
        "1 {} = {} {}",
        from.symbol,
        show(&convert(&one, from, to)?, 6),
        to.symbol
    ))
}

/// A recent conversion as listed: `42.195 km = 26.2188 mi`.
pub fn recent_line(value: &Num, from: &Unit, to: &Unit) -> Result<String, CalcError> {
    Ok(format!(
        "{} {} = {} {}",
        show(value, 10),
        from.symbol,
        show(&convert(value, from, to)?, 6),
        to.symbol
    ))
}

/// When the category changes, the unit of the same name in the new one (by
/// symbol), so a value keeps its meaning if the dimension matches - which in
/// this table means never; the default pair otherwise.
#[must_use]
pub fn units_after_category_change(old_from: &Unit, old_to: &Unit, new: &Category) -> (usize, usize) {
    let find = |u: &Unit| new.units.iter().position(|n| n.symbol == u.symbol);
    match (find(old_from), find(old_to)) {
        (Some(a), Some(b)) => (a, b),
        _ => new.default_pair,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(cat: &str, value: &str, from: &str, to: &str) -> String {
        let c = category(cat).unwrap();
        let f = c.units.iter().find(|u| u.symbol == from).unwrap();
        let t = c.units.iter().find(|u| u.symbol == to).unwrap();
        show(&convert(&Num::parse(value).unwrap(), f, t).unwrap(), 10)
    }

    #[test]
    fn the_marathon_sample_in_kilometres_and_miles() {
        assert_eq!(conv("Length", "42.195", "km", "mi"), "26.21875746");
        assert_eq!(conv("Length", "26.2187574564543", "mi", "km"), "42.195");
    }

    #[test]
    fn exact_factors_convert_exactly() {
        assert_eq!(conv("Length", "1", "mi", "m"), "1,609.344");
        assert_eq!(conv("Length", "12", "in", "ft"), "1");
        assert_eq!(conv("Mass", "1", "lb", "g"), "453.59237");
        assert_eq!(conv("Data", "1", "GiB", "MiB"), "1,024");
        assert_eq!(conv("Data", "8", "bit", "B"), "1");
        assert_eq!(conv("Time", "1", "d", "min"), "1,440");
        assert_eq!(conv("Speed", "36", "km/h", "m/s"), "10");
        assert_eq!(conv("Area", "1", "ha", "m\u{b2}"), "10,000");
        assert_eq!(conv("Volume", "1", "gal", "L"), "3.785411784");
    }

    #[test]
    fn temperatures_use_their_offsets() {
        assert_eq!(conv("Temperature", "100", "\u{b0}F", "\u{b0}C"), "37.77777778");
        assert_eq!(conv("Temperature", "0", "\u{b0}C", "\u{b0}F"), "32");
        assert_eq!(conv("Temperature", "100", "\u{b0}C", "\u{b0}F"), "212");
        assert_eq!(conv("Temperature", "-40", "\u{b0}C", "\u{b0}F"), "-40");
        assert_eq!(conv("Temperature", "0", "K", "\u{b0}C"), "-273.15");
    }

    #[test]
    fn converting_there_and_back_returns_the_value() {
        for c in &CATEGORIES {
            for a in c.units {
                for b in c.units {
                    let v = Num::parse("123.456").unwrap();
                    let there = convert(&v, a, b).unwrap();
                    let back = convert(&there, b, a).unwrap();
                    assert_eq!(show(&back, 12), "123.456", "{} {} -> {}", c.name, a.symbol, b.symbol);
                }
            }
        }
    }

    #[test]
    fn the_rate_and_recent_lines() {
        let len = category("length").unwrap();
        let (km, mi) = (&len.units[3], &len.units[7]);
        assert_eq!(rate_line(km, mi).unwrap(), "1 km = 0.621371 mi");
        assert_eq!(
            recent_line(&Num::parse("42.195").unwrap(), km, mi).unwrap(),
            "42.195 km = 26.2188 mi"
        );
        let t = category("Temperature").unwrap();
        assert_eq!(
            recent_line(&Num::parse("100").unwrap(), &t.units[1], &t.units[0]).unwrap(),
            "100 \u{b0}F = 37.7778 \u{b0}C"
        );
    }

    #[test]
    fn every_category_has_a_valid_default_pair_and_distinct_symbols() {
        for c in &CATEGORIES {
            assert!(c.default_pair.0 < c.units.len() && c.default_pair.1 < c.units.len(), "{}", c.name);
            assert_ne!(c.default_pair.0, c.default_pair.1, "{}", c.name);
            for (i, a) in c.units.iter().enumerate() {
                for b in &c.units[i + 1..] {
                    assert_ne!(a.symbol, b.symbol, "{}", c.name);
                }
                assert!(Num::parse(a.num).is_ok() && Num::parse(a.den).is_ok() && Num::parse(a.offset).is_ok());
            }
        }
        let names: Vec<&str> = CATEGORIES.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec!["Length", "Mass", "Temperature", "Area", "Volume", "Speed", "Data", "Time"]
        );
    }

    #[test]
    fn a_category_change_falls_back_to_the_default_pair() {
        let len = category("Length").unwrap();
        let mass = category("Mass").unwrap();
        assert_eq!(
            units_after_category_change(&len.units[3], &len.units[7], mass),
            mass.default_pair
        );
        assert_eq!(
            units_after_category_change(&len.units[3], &len.units[7], len),
            (3, 7)
        );
    }
}
