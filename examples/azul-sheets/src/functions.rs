//! The function catalogue: what the formula bar's autocomplete offers and
//! the FORMULAS tab's library inserts. IronCalc has no public function list
//! (its `functions` module is private), so the app ships one: the functions
//! a household budget, a small business sheet or a school assignment reach
//! for, with their signatures. Pure data; no azul.

/// A function's family (the FORMULAS tab's library groups).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Math,
    Statistical,
    Logical,
    Text,
    Lookup,
    DateTime,
    Financial,
    Information,
}

impl Category {
    /// Every category, in the library's order.
    pub const ALL: [Category; 8] = [
        Category::Math,
        Category::Statistical,
        Category::Logical,
        Category::Text,
        Category::Lookup,
        Category::DateTime,
        Category::Financial,
        Category::Information,
    ];

    /// The group's caption.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Category::Math => "Math & Trig",
            Category::Statistical => "Statistical",
            Category::Logical => "Logical",
            Category::Text => "Text",
            Category::Lookup => "Lookup & Reference",
            Category::DateTime => "Date & Time",
            Category::Financial => "Financial",
            Category::Information => "Information",
        }
    }
}

/// One function: its name, its arguments as Excel writes them and a line
/// about what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: &'static str,
    pub args: &'static str,
    pub category: Category,
    pub about: &'static str,
}

impl Function {
    /// "SUM(number1, [number2], ...)".
    #[must_use]
    pub fn signature(&self) -> String {
        format!("{}({})", self.name, self.args)
    }
}

const fn f(name: &'static str, args: &'static str, category: Category, about: &'static str) -> Function {
    Function {
        name,
        args,
        category,
        about,
    }
}

use Category::{DateTime, Financial, Information, Logical, Lookup, Math, Statistical, Text};

/// The catalogue, alphabetical within each category.
pub const FUNCTIONS: &[Function] = &[
    f("ABS", "number", Math, "The absolute value of a number."),
    f("CEILING", "number, significance", Math, "Rounds a number up to the nearest multiple."),
    f("EXP", "number", Math, "e raised to the power of a number."),
    f("FLOOR", "number, significance", Math, "Rounds a number down to the nearest multiple."),
    f("INT", "number", Math, "Rounds a number down to the nearest integer."),
    f("LN", "number", Math, "The natural logarithm of a number."),
    f("LOG10", "number", Math, "The base-10 logarithm of a number."),
    f("MOD", "number, divisor", Math, "The remainder after a division."),
    f("PI", "", Math, "The number pi."),
    f("POWER", "number, power", Math, "A number raised to a power."),
    f("PRODUCT", "number1, [number2], ...", Math, "Multiplies its arguments."),
    f("RAND", "", Math, "A random number between 0 and 1."),
    f("RANDBETWEEN", "bottom, top", Math, "A random integer between two numbers."),
    f("ROUND", "number, num_digits", Math, "Rounds a number to a number of digits."),
    f("ROUNDDOWN", "number, num_digits", Math, "Rounds a number down, toward zero."),
    f("ROUNDUP", "number, num_digits", Math, "Rounds a number up, away from zero."),
    f("SIGN", "number", Math, "The sign of a number: 1, 0 or -1."),
    f("SQRT", "number", Math, "The square root of a number."),
    f("SUM", "number1, [number2], ...", Math, "Adds its arguments."),
    f("SUMIF", "range, criteria, [sum_range]", Math, "Adds the cells that meet a condition."),
    f("SUMIFS", "sum_range, criteria_range1, criteria1, ...", Math, "Adds the cells that meet several conditions."),
    f("SUMPRODUCT", "array1, [array2], ...", Math, "The sum of the products of corresponding entries."),
    f("TRUNC", "number, [num_digits]", Math, "Cuts a number to an integer."),
    f("AVERAGE", "number1, [number2], ...", Statistical, "The arithmetic mean of its arguments."),
    f("AVERAGEIF", "range, criteria, [average_range]", Statistical, "The mean of the cells that meet a condition."),
    f("AVERAGEIFS", "average_range, criteria_range1, criteria1, ...", Statistical, "The mean of the cells that meet several conditions."),
    f("COUNT", "value1, [value2], ...", Statistical, "Counts the cells that hold numbers."),
    f("COUNTA", "value1, [value2], ...", Statistical, "Counts the cells that are not empty."),
    f("COUNTBLANK", "range", Statistical, "Counts the empty cells in a range."),
    f("COUNTIF", "range, criteria", Statistical, "Counts the cells that meet a condition."),
    f("COUNTIFS", "criteria_range1, criteria1, ...", Statistical, "Counts the cells that meet several conditions."),
    f("LARGE", "array, k", Statistical, "The k-th largest value."),
    f("MAX", "number1, [number2], ...", Statistical, "The largest value."),
    f("MAXIFS", "max_range, criteria_range1, criteria1, ...", Statistical, "The largest value among the cells that meet conditions."),
    f("MEDIAN", "number1, [number2], ...", Statistical, "The median of its arguments."),
    f("MIN", "number1, [number2], ...", Statistical, "The smallest value."),
    f("MINIFS", "min_range, criteria_range1, criteria1, ...", Statistical, "The smallest value among the cells that meet conditions."),
    f("SMALL", "array, k", Statistical, "The k-th smallest value."),
    f("STDEV.S", "number1, [number2], ...", Statistical, "The standard deviation of a sample."),
    f("AND", "logical1, [logical2], ...", Logical, "TRUE if every argument is TRUE."),
    f("FALSE", "", Logical, "The logical value FALSE."),
    f("IF", "logical_test, [value_if_true], [value_if_false]", Logical, "One value if a condition is TRUE, another if it is FALSE."),
    f("IFERROR", "value, value_if_error", Logical, "A value, or another one if it is an error."),
    f("IFS", "logical_test1, value_if_true1, ...", Logical, "The value of the first condition that is TRUE."),
    f("NOT", "logical", Logical, "Reverses a logical value."),
    f("OR", "logical1, [logical2], ...", Logical, "TRUE if any argument is TRUE."),
    f("SWITCH", "expression, value1, result1, ...", Logical, "The result matching the expression's value."),
    f("TRUE", "", Logical, "The logical value TRUE."),
    f("XOR", "logical1, [logical2], ...", Logical, "TRUE if an odd number of arguments is TRUE."),
    f("CONCAT", "text1, [text2], ...", Text, "Joins texts."),
    f("EXACT", "text1, text2", Text, "TRUE if two texts are identical."),
    f("FIND", "find_text, within_text, [start_num]", Text, "Where one text starts in another (case-sensitive)."),
    f("LEFT", "text, [num_chars]", Text, "The first characters of a text."),
    f("LEN", "text", Text, "The number of characters in a text."),
    f("LOWER", "text", Text, "A text in lower case."),
    f("MID", "text, start_num, num_chars", Text, "Characters from the middle of a text."),
    f("PROPER", "text", Text, "Capitalizes every word."),
    f("REPLACE", "old_text, start_num, num_chars, new_text", Text, "Replaces part of a text."),
    f("RIGHT", "text, [num_chars]", Text, "The last characters of a text."),
    f("SEARCH", "find_text, within_text, [start_num]", Text, "Where one text starts in another (any case)."),
    f("SUBSTITUTE", "text, old_text, new_text, [instance_num]", Text, "Replaces a text inside a text."),
    f("TEXT", "value, format_text", Text, "A number formatted as text."),
    f("TEXTJOIN", "delimiter, ignore_empty, text1, ...", Text, "Joins texts with a delimiter."),
    f("TRIM", "text", Text, "Removes the extra spaces."),
    f("UPPER", "text", Text, "A text in upper case."),
    f("VALUE", "text", Text, "The number a text spells."),
    f("CHOOSE", "index_num, value1, [value2], ...", Lookup, "The value at a position in a list."),
    f("COLUMN", "[reference]", Lookup, "The column number of a reference."),
    f("HLOOKUP", "lookup_value, table_array, row_index_num, [range_lookup]", Lookup, "Looks up a value in the top row of a table."),
    f("INDEX", "array, row_num, [column_num]", Lookup, "The value at a row and column of a range."),
    f("INDIRECT", "ref_text, [a1]", Lookup, "The reference a text names."),
    f("MATCH", "lookup_value, lookup_array, [match_type]", Lookup, "The position of a value in a range."),
    f("OFFSET", "reference, rows, cols, [height], [width]", Lookup, "A reference shifted from another."),
    f("ROW", "[reference]", Lookup, "The row number of a reference."),
    f("VLOOKUP", "lookup_value, table_array, col_index_num, [range_lookup]", Lookup, "Looks up a value in the first column of a table."),
    f("XLOOKUP", "lookup_value, lookup_array, return_array, ...", Lookup, "Looks up a value and returns its match."),
    f("DATE", "year, month, day", DateTime, "The date of a year, month and day."),
    f("DAY", "serial_number", DateTime, "The day of a date."),
    f("DAYS", "end_date, start_date", DateTime, "The days between two dates."),
    f("EDATE", "start_date, months", DateTime, "The date months before or after another."),
    f("EOMONTH", "start_date, months", DateTime, "The last day of a month."),
    f("HOUR", "serial_number", DateTime, "The hour of a time."),
    f("MONTH", "serial_number", DateTime, "The month of a date."),
    f("NETWORKDAYS", "start_date, end_date, [holidays]", DateTime, "The working days between two dates."),
    f("NOW", "", DateTime, "The current date and time."),
    f("TODAY", "", DateTime, "The current date."),
    f("WEEKDAY", "serial_number, [return_type]", DateTime, "The day of the week of a date."),
    f("YEAR", "serial_number", DateTime, "The year of a date."),
    f("FV", "rate, nper, pmt, [pv], [type]", Financial, "The future value of an investment."),
    f("IPMT", "rate, per, nper, pv, [fv], [type]", Financial, "The interest part of a payment."),
    f("NPER", "rate, pmt, pv, [fv], [type]", Financial, "The number of payment periods."),
    f("NPV", "rate, value1, [value2], ...", Financial, "The net present value of cash flows."),
    f("PMT", "rate, nper, pv, [fv], [type]", Financial, "The payment of a loan."),
    f("PV", "rate, nper, pmt, [fv], [type]", Financial, "The present value of an investment."),
    f("RATE", "nper, pmt, pv, [fv], [type], [guess]", Financial, "The interest rate per period."),
    f("ISBLANK", "value", Information, "TRUE if a cell is empty."),
    f("ISERROR", "value", Information, "TRUE if a value is an error."),
    f("ISNUMBER", "value", Information, "TRUE if a value is a number."),
    f("ISTEXT", "value", Information, "TRUE if a value is text."),
    f("NA", "", Information, "The error value #N/A."),
];

/// The functions whose name starts with `prefix` (any case), at most
/// `limit`, in catalogue order with exact-length matches first.
#[must_use]
pub fn by_prefix(prefix: &str, limit: usize) -> Vec<&'static Function> {
    let p = prefix.to_ascii_uppercase();
    if p.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<&'static Function> = FUNCTIONS.iter().filter(|f| f.name.starts_with(&p)).collect();
    found.sort_by_key(|f| (f.name.len() != p.len(), f.name.len(), f.name));
    found.truncate(limit);
    found
}

/// The functions of one category.
pub fn in_category(category: Category) -> impl Iterator<Item = &'static Function> {
    FUNCTIONS.iter().filter(move |f| f.category == category)
}

/// The function name being typed at `cursor` (in characters) of a
/// formula: `(start, prefix)` when `text` is a formula and the letters
/// before the cursor follow an operator, a parenthesis, a comma or the `=`
/// ("=SU" -> (1, "SU"), "=A1+ro" -> (4, "ro")). Not inside a string.
#[must_use]
pub fn typed_name(text: &str, cursor: usize) -> Option<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&'=') {
        return None;
    }
    let cursor = cursor.min(chars.len());
    if chars[..cursor].iter().filter(|c| **c == '"').count() % 2 == 1 {
        return None;
    }
    let mut start = cursor;
    while start > 1 && (chars[start - 1].is_ascii_alphabetic() || chars[start - 1] == '.') {
        start -= 1;
    }
    if start == cursor {
        return None;
    }
    let before = chars[start - 1];
    if !matches!(before, '=' | '(' | ',' | ';' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ' ') {
        return None;
    }
    // "A1" is a reference being typed, not a name: a digit right after the
    // letters means the user is past the name.
    if chars.get(cursor).is_some_and(char::is_ascii_digit) {
        return None;
    }
    Some((start, chars[start..cursor].iter().collect()))
}

/// `text` with the name typed at `cursor` completed to `name(`: the new
/// text and the new cursor (after the parenthesis).
#[must_use]
pub fn complete(text: &str, cursor: usize, name: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let start = typed_name(text, cursor).map_or(cursor, |(s, _)| s);
    let mut out: String = chars[..start].iter().collect();
    out.push_str(name);
    out.push('(');
    let at = out.chars().count();
    out.extend(chars[cursor..].iter());
    (out, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_is_listed_once_and_in_a_category() {
        let mut names: Vec<&str> = FUNCTIONS.iter().map(|f| f.name).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n, "a name listed twice");
        for c in Category::ALL {
            assert!(in_category(c).next().is_some(), "{} is empty", c.label());
        }
    }

    #[test]
    fn a_prefix_finds_its_functions_shortest_first_in_any_case() {
        let names: Vec<&str> = by_prefix("su", 3).iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["SUM", "SUMIF", "SUMIFS"]);
        assert!(by_prefix("", 5).is_empty());
        assert!(by_prefix("zzz", 5).is_empty());
        assert_eq!(by_prefix("SUM", 1)[0].signature(), "SUM(number1, [number2], ...)");
    }

    #[test]
    fn the_name_being_typed_is_found_after_an_operator_and_not_in_a_string_or_a_reference() {
        assert_eq!(typed_name("=SU", 3), Some((1, String::from("SU"))));
        assert_eq!(typed_name("=A1+ro", 6), Some((4, String::from("ro"))));
        assert_eq!(typed_name("=IF(av", 6), Some((4, String::from("av"))));
        assert_eq!(typed_name("SU", 2), None, "not a formula");
        assert_eq!(typed_name("=\"SU", 4), None, "inside a string");
        assert_eq!(typed_name("=SUM(", 5), None, "past the name");
    }

    #[test]
    fn completing_replaces_the_prefix_with_the_name_and_an_open_parenthesis() {
        assert_eq!(complete("=su", 3, "SUM"), (String::from("=SUM("), 5));
        assert_eq!(complete("=1+av*2", 5, "AVERAGE"), (String::from("=1+AVERAGE(*2"), 11));
    }
}
