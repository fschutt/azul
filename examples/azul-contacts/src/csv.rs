//! Contacts from a CSV file - what Outlook ("Export to a file > Comma Separated Values") and
//! Google Contacts ("Export > Google CSV") write: a header row naming the columns, one person
//! per row. Each column is MAPPED to a contact field; the mapping starts from the header's
//! name (`guess`, the names both write), and the import preview lets the user change it.
//!
//! The file: RFC 4180 (fields in double quotes may hold the separator, line breaks and `""`
//! for a quote), CRLF or LF, a leading byte-order mark dropped, the separator - comma,
//! semicolon (Excel in many locales) or tab - the one the header line holds most of.

use crate::contact::{Address, Birthday, Contact, Labeled};

/// What a CSV column becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// Not imported.
    Skip,
    Given,
    Family,
    /// A whole name ("Ada Lovelace"): the last word is the family name.
    FullName,
    Nickname,
    Email,
    MobilePhone,
    WorkPhone,
    HomePhone,
    Org,
    Department,
    Title,
    Birthday,
    Street,
    City,
    Region,
    PostalCode,
    Country,
    Url,
    Notes,
    /// Groups, separated by `;` or `:::` (Google's).
    Groups,
}

impl Field {
    /// Every choice, in the order the mapping control lists them.
    pub const ALL: [Field; 21] = [
        Field::Skip,
        Field::Given,
        Field::Family,
        Field::FullName,
        Field::Nickname,
        Field::Email,
        Field::MobilePhone,
        Field::WorkPhone,
        Field::HomePhone,
        Field::Org,
        Field::Department,
        Field::Title,
        Field::Birthday,
        Field::Street,
        Field::City,
        Field::Region,
        Field::PostalCode,
        Field::Country,
        Field::Url,
        Field::Notes,
        Field::Groups,
    ];

    /// What the mapping control says.
    #[must_use]
    pub fn label(self) -> &'static str {
        let _ = self;
        todo!()
    }

    /// The position in [`Field::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Field::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// A CSV file: its header and its rows (each as long as the header).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Reads a CSV text; `Err` says why it is no table (no header, a quote left open).
pub fn parse(text: &str) -> Result<Table, String> {
    let _ = text;
    todo!()
}

/// The field a column named `header` most likely holds (Outlook's and Google's names, any
/// case); `Skip` for one nobody knows.
#[must_use]
pub fn guess(header: &str) -> Field {
    let _ = header;
    todo!()
}

/// The contacts of `table` with each column mapped by `mapping` (by position; a column
/// without one is skipped), and what could not be read. A row with no name, e-mail or phone
/// is left out (an empty line of a spreadsheet).
#[must_use]
pub fn contacts(table: &Table, mapping: &[Field]) -> (Vec<Contact>, Vec<String>) {
    let _ = (table, mapping);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTLOOK: &str = "\u{feff}First Name,Last Name,Company,Job Title,E-mail Address,\
        Mobile Phone,Business Phone,Business Street,Business City,Business Postal Code,\
        Business Country/Region,Birthday,Notes,Categories\r\n\
        Ada,Lovelace,Analytical Engines,Programmer,ada@example.org,+44 20 1234,\
        +44 20 9876,\"12 St James's Square\",London,SW1Y 4JH,United Kingdom,10.12.1815,\
        \"First program, \"\"Note G\"\"\nfor the Engine\",Friends;Work\r\n\
        ,,,,,,,,,,,,,\r\n\
        Charles,,,,,,,,,,,,,\r\n";

    #[test]
    fn a_csv_file_is_read_with_quotes_line_breaks_and_its_separator() {
        let t = parse(OUTLOOK).unwrap();
        assert_eq!(t.headers.len(), 14);
        assert_eq!(t.headers[0], "First Name", "the byte-order mark goes");
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[0][7], "12 St James's Square");
        assert_eq!(t.rows[0][12], "First program, \"Note G\"\nfor the Engine");
        // Excel's semicolons; a short row is filled up; LF line ends.
        let t = parse("Name;E-mail\nAda Lovelace;ada@example.org\nBo\n").unwrap();
        assert_eq!(t.headers, ["Name", "E-mail"]);
        assert_eq!(t.rows, [vec!["Ada Lovelace", "ada@example.org"], vec!["Bo", ""]]);
        let t = parse("Name\tEmail\r\nAda\tada@example.org").unwrap();
        assert_eq!(t.rows, [vec!["Ada", "ada@example.org"]]);
        assert!(parse("").is_err(), "no header");
        assert!(parse("Name\n\"Ada").is_err(), "a quote left open");
    }

    #[test]
    fn the_columns_outlook_and_google_write_are_mapped_by_their_names() {
        for (header, field) in [
            ("First Name", Field::Given),
            ("Given Name", Field::Given),
            ("last name", Field::Family),
            ("Family Name", Field::Family),
            ("Name", Field::FullName),
            ("Display Name", Field::FullName),
            ("Nickname", Field::Nickname),
            ("E-mail Address", Field::Email),
            ("E-mail 1 - Value", Field::Email),
            ("Email", Field::Email),
            ("Mobile Phone", Field::MobilePhone),
            ("Business Phone", Field::WorkPhone),
            ("Home Phone", Field::HomePhone),
            ("Phone 1 - Value", Field::MobilePhone),
            ("Company", Field::Org),
            ("Organization 1 - Name", Field::Org),
            ("Department", Field::Department),
            ("Job Title", Field::Title),
            ("Birthday", Field::Birthday),
            ("Business Street", Field::Street),
            ("Home City", Field::City),
            ("Business State", Field::Region),
            ("Business Postal Code", Field::PostalCode),
            ("Business Country/Region", Field::Country),
            ("Web Page", Field::Url),
            ("Notes", Field::Notes),
            ("Categories", Field::Groups),
            ("Group Membership", Field::Groups),
            ("E-mail Type", Field::Skip),
            ("Assistant's Name", Field::Skip),
            ("Something else", Field::Skip),
        ] {
            assert_eq!(guess(header), field, "{header:?}");
        }
        for f in Field::ALL {
            assert_eq!(Field::ALL[f.index()], f);
            assert!(!f.label().is_empty());
        }
    }

    #[test]
    fn each_row_becomes_a_contact_by_the_mapping() {
        let t = parse(OUTLOOK).unwrap();
        let mapping: Vec<Field> = t.headers.iter().map(|h| guess(h)).collect();
        let (cs, problems) = contacts(&t, &mapping);
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(cs.len(), 2, "the empty row is left out");
        let ada = &cs[0];
        assert_eq!((ada.given.as_str(), ada.family.as_str()), ("Ada", "Lovelace"));
        assert_eq!((ada.org.as_str(), ada.title.as_str()), ("Analytical Engines", "Programmer"));
        assert_eq!(ada.emails, [Labeled::new("work", "ada@example.org")]);
        assert_eq!(
            ada.phones,
            [Labeled::new("mobile", "+44 20 1234"), Labeled::new("work", "+44 20 9876")]
        );
        assert_eq!(ada.addresses.len(), 1);
        assert_eq!(ada.addresses[0].street, "12 St James's Square");
        assert_eq!(ada.addresses[0].locality, "London");
        assert_eq!(ada.addresses[0].postcode, "SW1Y 4JH");
        assert_eq!(ada.addresses[0].country, "United Kingdom");
        assert_eq!(ada.birthday, Birthday::parse("10.12.1815"));
        assert!(ada.notes.contains("Note G"));
        assert_eq!(ada.groups, ["Friends", "Work"]);
        assert_eq!(cs[1].given, "Charles");

        // The user maps the columns otherwise: the company is skipped, a full name is split.
        let t = parse("Name,Company\nAda King Lovelace,Engines\n").unwrap();
        let (cs, _) = contacts(&t, &[Field::FullName, Field::Skip]);
        assert_eq!((cs[0].given.as_str(), cs[0].family.as_str()), ("Ada King", "Lovelace"));
        assert_eq!(cs[0].org, "");
        // A birthday that is no date is said, the person kept.
        let t = parse("First Name,Birthday\nAda,someday\n").unwrap();
        let (cs, problems) = contacts(&t, &[Field::Given, Field::Birthday]);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].birthday, None);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("someday"), "{problems:?}");
    }
}
