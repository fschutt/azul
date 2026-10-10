//! Contacts from a CSV file - what Outlook ("Export to a file > Comma Separated Values") and
//! Google Contacts ("Export > Google CSV") write: a header row naming the columns, one person
//! per row. Each column is MAPPED to a contact field; the mapping starts from the header's
//! name (`guess`, the names both write), and the import preview lets the user change it.
//!
//! The file is read by the Azlin apps' one CSV reader (`azul_appkit::csv`): RFC 4180 (fields
//! in double quotes may hold the separator, line breaks and `""` for a quote), CRLF or LF, a
//! leading byte-order mark dropped, the separator - comma, semicolon (Excel in many locales)
//! or tab - the one the header line holds most of.

use azul_appkit::phrase::{Phrase, Text};

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

    /// What the mapping control says: a key of AzContacts' resources.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Field::Skip => "azcontacts-field-skip",
            Field::Given => "azcontacts-first-name",
            Field::Family => "azcontacts-last-name",
            Field::FullName => "azcontacts-field-full-name",
            Field::Nickname => "azcontacts-nickname",
            Field::Email => "azcontacts-field-email",
            Field::MobilePhone => "azcontacts-field-mobile-phone",
            Field::WorkPhone => "azcontacts-field-work-phone",
            Field::HomePhone => "azcontacts-field-home-phone",
            Field::Org => "azcontacts-company",
            Field::Department => "azcontacts-department",
            Field::Title => "azcontacts-job-title",
            Field::Birthday => "azcontacts-birthday",
            Field::Street => "azcontacts-street",
            Field::City => "azcontacts-city",
            Field::Region => "azcontacts-field-region",
            Field::PostalCode => "azcontacts-field-postal-code",
            Field::Country => "azcontacts-country",
            Field::Url => "azcontacts-field-web-page",
            Field::Notes => "azcontacts-notes",
            Field::Groups => "azcontacts-groups",
        }
    }

    /// The position in [`Field::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Field::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// A CSV file: its header and its rows (each as long as the header).
pub use azul_appkit::csv::Table;

/// Reads a CSV text with the apps' one reader ([`azul_appkit::csv::read_table`]); `Err` says
/// why it is no table (no header row).
pub fn parse(text: &str) -> Result<Table, String> {
    azul_appkit::csv::read_table(text)
}

/// The field a column named `header` most likely holds (Outlook's and Google's names, any
/// case); `Skip` for one nobody knows.
#[must_use]
pub fn guess(header: &str) -> Field {
    let h = header.trim().to_lowercase();
    let has = |w: &str| h.contains(w);
    match h.as_str() {
        "first name" | "given name" | "firstname" => return Field::Given,
        "last name" | "family name" | "surname" | "lastname" => return Field::Family,
        "name" | "full name" | "display name" => return Field::FullName,
        "nickname" => return Field::Nickname,
        "company" | "organization" | "organisation" => return Field::Org,
        "job title" | "organization 1 - title" => return Field::Title,
        "birthday" => return Field::Birthday,
        "notes" | "note" => return Field::Notes,
        "categories" | "group membership" | "groups" => return Field::Groups,
        _ => {}
    }
    // Not the person's own: a value's type or label, the assistant's, the manager's, a fax.
    if ["type", "assistant", "manager", "spouse", "children", "display", "fax", "label"]
        .iter()
        .any(|w| has(w))
    {
        return Field::Skip;
    }
    if has("e-mail") || has("email") {
        Field::Email
    } else if has("mobile") || has("cell") {
        Field::MobilePhone
    } else if has("phone") {
        if has("business") || has("work") || has("company") {
            Field::WorkPhone
        } else if has("home") {
            Field::HomePhone
        } else {
            Field::MobilePhone
        }
    } else if has("organization") && has("name") {
        Field::Org
    } else if has("department") {
        Field::Department
    } else if has("street") {
        Field::Street
    } else if has("city") {
        Field::City
    } else if has("country") {
        Field::Country
    } else if has("state") || has("region") {
        Field::Region
    } else if has("postal") || has("zip") || has("postcode") {
        Field::PostalCode
    } else if has("web page") || has("website") || has("url") {
        Field::Url
    } else {
        Field::Skip
    }
}

/// The label of a value from a column named `header`: `home` / `other`, else `work`.
fn label_of(header: &str) -> &'static str {
    let h = header.to_lowercase();
    if h.contains("home") || h.contains("personal") {
        "home"
    } else if h.contains("other") {
        "other"
    } else {
        "work"
    }
}

/// The contacts of `table` with each column mapped by `mapping` (by position; a column
/// without one is skipped), and what could not be read. A row with no name, e-mail or phone
/// is left out (an empty line of a spreadsheet).
#[must_use]
pub fn contacts(table: &Table, mapping: &[Field]) -> (Vec<Contact>, Vec<Text>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for (n, row) in table.rows.iter().enumerate() {
        let mut c = Contact::default();
        // One address per label (a home and a business address are two).
        let mut addresses: Vec<Address> = Vec::new();
        for (i, value) in row.iter().enumerate() {
            let v = value.trim();
            let field = mapping.get(i).copied().unwrap_or(Field::Skip);
            if v.is_empty() || field == Field::Skip {
                continue;
            }
            let header = table.headers.get(i).map(String::as_str).unwrap_or_default();
            let mut address = |f: &dyn Fn(&mut Address)| {
                let label = label_of(header);
                let at = match addresses.iter().position(|a| a.label == label) {
                    Some(at) => at,
                    None => {
                        addresses.push(Address {
                            label: label.to_string(),
                            ..Address::default()
                        });
                        addresses.len() - 1
                    }
                };
                f(&mut addresses[at]);
            };
            match field {
                Field::Skip => {}
                Field::Given => c.given = v.to_string(),
                Field::Family => c.family = v.to_string(),
                Field::FullName => {
                    // Only where no first / last name column said it.
                    if c.given.is_empty() && c.family.is_empty() {
                        match v.rsplit_once(char::is_whitespace) {
                            Some((given, family)) => {
                                c.given = given.trim().to_string();
                                c.family = family.trim().to_string();
                            }
                            None => c.given = v.to_string(),
                        }
                    }
                }
                Field::Nickname => c.nickname = v.to_string(),
                Field::Email => c.emails.push(Labeled::new(label_of(header), v)),
                Field::MobilePhone => c.phones.push(Labeled::new("mobile", v)),
                Field::WorkPhone => c.phones.push(Labeled::new("work", v)),
                Field::HomePhone => c.phones.push(Labeled::new("home", v)),
                Field::Org => c.org = v.to_string(),
                Field::Department => c.department = v.to_string(),
                Field::Title => c.title = v.to_string(),
                Field::Birthday => match Birthday::parse(v) {
                    Some(b) => c.birthday = Some(b),
                    None => problems.push(
                        Phrase::new("azcontacts-csv-birthday-no-date")
                            .arg("row", n + 2)
                            .arg("value", v)
                            .into(),
                    ),
                },
                Field::Street => address(&|a| a.street = v.to_string()),
                Field::City => address(&|a| a.locality = v.to_string()),
                Field::Region => address(&|a| a.region = v.to_string()),
                Field::PostalCode => address(&|a| a.postcode = v.to_string()),
                Field::Country => address(&|a| a.country = v.to_string()),
                Field::Url => c.urls.push(Labeled::new(label_of(header), v)),
                Field::Notes => {
                    if !c.notes.is_empty() {
                        c.notes.push('\n');
                    }
                    c.notes.push_str(value.trim_end());
                }
                Field::Groups => {
                    for g in v.split(":::").flat_map(|g| g.split(';')) {
                        let g = g.trim();
                        // Google's own groups ("* myContacts") are no groups of the user's.
                        if !g.is_empty()
                            && !g.starts_with('*')
                            && !c.groups.iter().any(|x| x.eq_ignore_ascii_case(g))
                        {
                            c.groups.push(g.to_string());
                        }
                    }
                }
            }
        }
        c.addresses = addresses.into_iter().filter(|a| !a.is_empty()).collect();
        let named = !(c.given.is_empty() && c.family.is_empty() && c.org.is_empty());
        if named || !c.emails.is_empty() || !c.phones.is_empty() {
            out.push(c);
        }
    }
    (out, problems)
}

#[cfg(test)]
mod tests {
    use azul_appkit::phrase::{Arg, Text};

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
        // A quote left open reads to the end of the file (the csv crate does not report it).
        let t = parse("Name\n\"Ada").unwrap();
        assert_eq!(t.rows, [vec!["Ada"]]);
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
            assert!(f.label().starts_with("azcontacts-"), "a key: {}", f.label());
        }
    }

    #[test]
    fn each_row_becomes_a_contact_by_the_mapping() {
        let t = parse(OUTLOOK).unwrap();
        let mapping: Vec<Field> = t.headers.iter().map(|h| guess(h)).collect();
        let (cs, problems) = contacts(&t, &mapping);
        assert_eq!(problems, Vec::<Text>::new());
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
        assert_eq!(
            problems[0].phrase("azcontacts-csv-birthday-no-date").and_then(|p| p.get("value")),
            Some(&Arg::from("someday")),
            "{problems:?}"
        );
    }
}
