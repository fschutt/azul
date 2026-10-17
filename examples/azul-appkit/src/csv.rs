//! The one CSV reader of the Azlin apps (AzContacts, AzKeys, AzERP import what
//! spreadsheets, address books and password managers export).
//!
//! RFC 4180 through the `csv` crate: fields in double quotes may hold the
//! separator, line breaks and `""` for a quote; CRLF or LF line ends (a CRLF
//! inside a quoted field is one line break, `\n`); a leading byte-order mark
//! dropped; the separator - comma, semicolon (Excel in many locales) or tab -
//! the one the header line holds most of ([`separator`]). The first row is the
//! header; every other row is as long as the header (a short row is filled up
//! with empty cells, a long one cut). A quote left open reads to the end of the
//! file (the `csv` crate does not report it).

/// A CSV file: its header and its rows (each as long as the header).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// The header cells, trimmed.
    pub headers: Vec<String>,
    /// The rows after the header; a blank line is no row.
    pub rows: Vec<Vec<String>>,
}

/// The separator a header line holds most of: comma, semicolon or tab (a comma
/// on a tie).
#[must_use]
pub fn separator(header_line: &str) -> u8 {
    let mut best = (b',', 0usize);
    for sep in [b',', b';', b'\t'] {
        let n = header_line.bytes().filter(|b| *b == sep).count();
        if n > best.1 {
            best = (sep, n);
        }
    }
    best.0
}

/// Reads a CSV text; `Err` says why it is no table (no header row, a row the
/// reader cannot read).
pub fn read_table(text: &str) -> Result<Table, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.lines().next().unwrap_or_default();
    if first.trim().is_empty() {
        return Err(String::from("The file is empty: it has no header row."));
    }
    let mut reader = ::csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .delimiter(separator(first))
        .from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| format!("The header row cannot be read: {e}"))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    if headers.iter().all(String::is_empty) {
        return Err(String::from("The file has no header row."));
    }
    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| format!("Row {} cannot be read: {e}", i + 2))?;
        // A blank line is no row (the reader drops empty ones itself).
        if record.len() == 1 && record[0].trim().is_empty() {
            continue;
        }
        let mut row: Vec<String> = record.iter().map(one_line_break).collect();
        row.resize(headers.len(), String::new());
        rows.push(row);
    }
    Ok(Table { headers, rows })
}

/// A cell with each CRLF inside it as one line break.
fn one_line_break(cell: &str) -> String {
    if cell.contains('\r') {
        cell.replace("\r\n", "\n")
    } else {
        cell.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_separator_is_the_one_the_header_line_holds_most_of_and_a_comma_on_a_tie() {
        assert_eq!(separator("a,b,c"), b',');
        assert_eq!(separator("a;b;c"), b';');
        assert_eq!(separator("a\tb\tc"), b'\t');
        assert_eq!(separator("\"a,b\";c;d"), b';');
        assert_eq!(separator("a,b;c"), b',', "a tie is a comma");
        assert_eq!(separator("name"), b',', "none at all is a comma");
    }

    #[test]
    fn a_csv_file_is_read_with_quotes_line_breaks_a_bom_and_its_separator() {
        let text = "\u{feff}Number;Name;Note\r\nA-1;\"Desk; oak\";\"Line 1\r\nLine \"\"2\"\"\"\r\nA-2;Chair\r\n";
        let t = read_table(text).unwrap();
        assert_eq!(
            t.headers,
            ["Number", "Name", "Note"],
            "the byte-order mark goes"
        );
        assert_eq!(t.rows.len(), 2);
        assert_eq!(
            t.rows[0],
            ["A-1", "Desk; oak", "Line 1\nLine \"2\""],
            "a CRLF inside a quoted field is one line break"
        );
        assert_eq!(t.rows[1], ["A-2", "Chair", ""], "a short row is filled up");
        let t = read_table("Name\tEmail\r\nAda\tada@example.org").unwrap();
        assert_eq!(
            t.rows,
            [vec!["Ada", "ada@example.org"]],
            "no line end at the end"
        );
        let t = read_table("a,b\n1,2,3\n").unwrap();
        assert_eq!(t.rows, [vec!["1", "2"]], "a long row is cut to the header");
    }

    #[test]
    fn header_cells_are_trimmed_and_blank_lines_are_no_rows() {
        let t = read_table(" Name , E-mail \n\nAda,ada@example.org\n   \n,\n").unwrap();
        assert_eq!(t.headers, ["Name", "E-mail"]);
        assert_eq!(
            t.rows,
            [vec!["Ada", "ada@example.org"], vec!["", ""]],
            "a row of empty cells is a row; an empty or blank line is none"
        );
    }

    #[test]
    fn a_file_without_a_header_row_is_no_table() {
        assert!(read_table("").is_err());
        assert!(read_table("\u{feff}  \n").is_err());
        assert!(
            read_table("\n1,2\n").is_err(),
            "the first line is the header"
        );
        assert!(
            read_table(",,\n1,2,3\n").is_err(),
            "a header of empty cells"
        );
    }

    #[test]
    fn a_quote_left_open_reads_to_the_end_of_the_file() {
        let t = read_table("Name,Note\nAda,\"open\nstill open\n").unwrap();
        assert_eq!(t.rows, [vec!["Ada", "open\nstill open\n"]]);
    }
}
