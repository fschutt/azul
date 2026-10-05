//! Import: the CSV files browsers and password managers export, and Bitwarden's JSON export.
//!
//! CSV (read with the apps' one reader, `azul_appkit::csv`: quotes, separators and line breaks
//! in fields, a byte-order mark): the columns are found by their header names, so one reader takes
//! - Chrome / Edge / Brave / Opera: `name,url,username,password,note`
//! - Firefox: `url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,
//!   timePasswordChanged` (milliseconds)
//! - Safari / macOS Passwords: `Title,URL,Username,Password,Notes,OTPAuth`
//! - Bitwarden: `folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,
//!   login_password,login_totp`
//! - 1Password: `Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes`
//! - KeePassXC: `Group,Title,Username,Password,URL,Notes,TOTP,Icon,Last Modified,Created`
//! - LastPass: `url,username,password,totp,extra,name,grouping,fav`
//!
//! Bitwarden JSON (`"encrypted": false`): logins, secure notes, cards, identities and SSH keys
//! with folders, favourites, custom fields and the password history. A password-protected or
//! account-encrypted export is refused with what to do instead.
//!
//! A row that has nothing to keep is skipped with its row number and the reason - never with a
//! value from the file.

use azul_storage::time::parse_iso8601;
use serde_json::Value;

use crate::vault::{host_of, Card, Field, Item, Kind, PastPassword, Vault};

/// Where a file came from (its header says it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Chrome,
    Firefox,
    Safari,
    BitwardenCsv,
    BitwardenJson,
    OnePassword,
    KeePassXc,
    LastPass,
    /// A CSV with the usual column names, from somewhere else.
    Csv,
}

impl Format {
    /// The name the import preview shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Format::Chrome => "Chrome / Edge CSV",
            Format::Firefox => "Firefox CSV",
            Format::Safari => "Safari / Passwords CSV",
            Format::BitwardenCsv => "Bitwarden CSV",
            Format::BitwardenJson => "Bitwarden JSON",
            Format::OnePassword => "1Password CSV",
            Format::KeePassXc => "KeePassXC CSV",
            Format::LastPass => "LastPass CSV",
            Format::Csv => "CSV",
        }
    }
}

/// What a file holds: its format, the items to add, and the rows skipped (one sentence each).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub format: Format,
    pub items: Vec<Item>,
    pub skipped: Vec<String>,
}

/// The header names of a CSV, lower case and trimmed.
fn normalized(headers: &[String]) -> Vec<String> {
    headers.iter().map(|h| h.trim().to_lowercase()).collect()
}

/// The format a CSV header names.
#[must_use]
pub fn detect(headers: &[String]) -> Format {
    let h = normalized(headers);
    let has = |name: &str| h.iter().any(|x| x == name);
    if has("httprealm") || has("formactionorigin") || has("timepasswordchanged") {
        Format::Firefox
    } else if has("login_password") || has("login_uri") || has("login_username") {
        Format::BitwardenCsv
    } else if has("otpauth") && (has("favorite") || has("archived") || has("tags")) {
        Format::OnePassword
    } else if has("otpauth") {
        Format::Safari
    } else if has("group") && has("title") && (has("last modified") || has("icon")) {
        Format::KeePassXc
    } else if has("grouping") || (has("extra") && has("fav")) {
        Format::LastPass
    } else if has("name") && has("url") && has("username") && has("password") {
        Format::Chrome
    } else {
        Format::Csv
    }
}

/// Where each kind of value is in a CSV row.
#[derive(Default)]
struct Columns {
    title: Option<usize>,
    url: Option<usize>,
    username: Option<usize>,
    password: Option<usize>,
    notes: Option<usize>,
    totp: Option<usize>,
    folder: Option<usize>,
    favorite: Option<usize>,
    tags: Option<usize>,
    kind: Option<usize>,
    fields: Option<usize>,
    /// Milliseconds since 1970 (Firefox).
    created_ms: Option<usize>,
    changed_ms: Option<usize>,
    /// ISO 8601 (KeePassXC).
    created_iso: Option<usize>,
    modified_iso: Option<usize>,
}

impl Columns {
    fn of(headers: &[String]) -> Columns {
        let h = normalized(headers);
        let find = |names: &[&str]| h.iter().position(|x| names.contains(&x.as_str()));
        Columns {
            title: find(&["name", "title", "item name", "account"]),
            url: find(&[
                "url",
                "login_uri",
                "website",
                "uri",
                "web site",
                "login url",
            ]),
            username: find(&[
                "username",
                "login_username",
                "user name",
                "login name",
                "login",
                "user",
            ])
            .or_else(|| find(&["email", "e-mail"])),
            password: find(&["password", "login_password"]),
            notes: find(&["note", "notes", "extra", "comments", "comment"]),
            totp: find(&[
                "totp",
                "otpauth",
                "login_totp",
                "otp",
                "one-time password",
                "2fa",
            ]),
            folder: find(&["folder", "group", "grouping"]),
            favorite: find(&["favorite", "favourite", "fav"]),
            tags: find(&["tags", "tag", "labels"]),
            kind: find(&["type"]),
            fields: find(&["fields"]),
            created_ms: find(&["timecreated"]),
            changed_ms: find(&["timepasswordchanged"]),
            created_iso: find(&["created", "creation date", "creationdate"]),
            modified_iso: find(&["last modified", "modified", "lastmodified"]),
        }
    }

    /// Whether the file has a column worth importing.
    fn any(&self) -> bool {
        self.title.is_some()
            || self.url.is_some()
            || self.username.is_some()
            || self.password.is_some()
            || self.notes.is_some()
    }
}

/// A yes in a CSV cell: `1`, `true`, `yes`, `y`, `x`.
fn truthy(cell: &str) -> bool {
    matches!(
        cell.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "x"
    )
}

/// The kind a `type` cell names (Bitwarden's words); a login when it names none.
fn kind_of(cell: &str) -> Kind {
    match cell
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "")
        .as_str()
    {
        "note" | "securenote" => Kind::Note,
        "card" => Kind::Card,
        "identity" => Kind::Identity,
        "sshkey" => Kind::SshKey,
        _ => Kind::Login,
    }
}

/// Bitwarden's `fields` cell: one `name: value` per line.
fn fields_of(cell: &str) -> Vec<Field> {
    cell.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| match line.split_once(": ") {
            Some((name, value)) => Field {
                name: name.trim().to_string(),
                value: value.to_string(),
                hidden: false,
            },
            None => Field {
                name: line.trim_end_matches(':').to_string(),
                value: String::new(),
                hidden: false,
            },
        })
        .collect()
}

/// Seconds of a milliseconds cell.
fn millis(cell: &str) -> Option<u64> {
    cell.trim()
        .parse::<u64>()
        .ok()
        .filter(|ms| *ms > 0)
        .map(|ms| ms / 1000)
}

/// The items of a CSV export; `Err` when the file is no table or has no column to import.
pub fn import_csv(text: &str, now: u64) -> Result<Imported, String> {
    let table = azul_appkit::csv::read_table(text)?;
    let headers = &table.headers;
    let columns = Columns::of(headers);
    if !columns.any() {
        return Err(
            "No column of this file is one AzKeys imports: it needs a header row naming \
                    name or title, url, username, password or notes."
                .to_string(),
        );
    }
    let format = detect(headers);
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for (n, record) in table.rows.iter().enumerate() {
        // The row's number as a spreadsheet shows it: the header is row 1.
        let row = n + 2;
        let get = |c: Option<usize>| {
            c.and_then(|i| record.get(i))
                .map(|cell| cell.trim())
                .unwrap_or("")
        };
        let (title, url, username, password) = (
            get(columns.title),
            get(columns.url),
            get(columns.username),
            get(columns.password),
        );
        let notes = get(columns.notes);
        let totp = get(columns.totp);
        if [title, url, username, password, notes, totp]
            .iter()
            .all(|s| s.is_empty())
        {
            skipped.push(format!(
                "row {row}: nothing to import (no name, website, user name, password or note)"
            ));
            continue;
        }
        // LastPass writes its secure notes as rows with the url `http://sn`.
        let lastpass_note = url.eq_ignore_ascii_case("http://sn");
        let kind = if lastpass_note {
            Kind::Note
        } else {
            kind_of(get(columns.kind))
        };
        let title = if title.is_empty() {
            host_of(url)
                .or_else(|| (!username.is_empty()).then(|| username.to_string()))
                .unwrap_or_else(|| "Imported item".to_string())
        } else {
            title.to_string()
        };
        let mut item = Item::new(kind, &title, now);
        if !url.is_empty() && !lastpass_note {
            item.urls.push(url.to_string());
        }
        item.username = username.to_string();
        item.password = password.to_string();
        item.notes = notes.to_string();
        item.totp = totp.to_string();
        item.folder = get(columns.folder).to_string();
        item.favorite = truthy(get(columns.favorite));
        for tag in get(columns.tags).split([',', ';']) {
            item.add_tag(tag);
        }
        item.fields = fields_of(get(columns.fields));
        if let Some(created) =
            millis(get(columns.created_ms)).or_else(|| parse_iso8601(get(columns.created_iso)))
        {
            item.created = created;
        }
        if let Some(modified) = parse_iso8601(get(columns.modified_iso)) {
            item.modified = modified;
        }
        if let Some(changed) = millis(get(columns.changed_ms)) {
            item.password_changed = changed;
        }
        items.push(item);
    }
    Ok(Imported {
        format,
        items,
        skipped,
    })
}

/// A JSON value as text: a string as it is, a number or a bool written out, else "".
fn text_of(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// A Bitwarden date (`2024-01-01T00:00:00.000Z`) as seconds since 1970.
fn date_of(value: &Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(parse_iso8601)
}

/// The identity fields of a Bitwarden export, in the order the item shows them:
/// `(key, label, hidden)`.
const IDENTITY_FIELDS: [(&str, &str, bool); 18] = [
    ("title", "Title", false),
    ("firstName", "First name", false),
    ("middleName", "Middle name", false),
    ("lastName", "Last name", false),
    ("company", "Company", false),
    ("email", "Email", false),
    ("phone", "Phone", false),
    ("address1", "Address", false),
    ("address2", "Address 2", false),
    ("address3", "Address 3", false),
    ("city", "City", false),
    ("state", "State", false),
    ("postalCode", "Postal code", false),
    ("country", "Country", false),
    ("username", "User name", false),
    ("ssn", "SSN", true),
    ("passportNumber", "Passport number", true),
    ("licenseNumber", "License number", true),
];

/// The SSH key fields of a Bitwarden export: `(key, label, hidden)`.
const SSH_FIELDS: [(&str, &str, bool); 3] = [
    ("privateKey", "Private key", true),
    ("publicKey", "Public key", false),
    ("keyFingerprint", "Fingerprint", false),
];

/// The fields of `object` named in `table`, those with a value.
fn fields_from(object: Option<&Value>, table: &[(&str, &str, bool)]) -> Vec<Field> {
    let Some(object) = object else {
        return Vec::new();
    };
    table
        .iter()
        .filter_map(|(key, label, hidden)| {
            let value = text_of(object, key);
            (!value.trim().is_empty()).then(|| Field {
                name: (*label).to_string(),
                value,
                hidden: *hidden,
            })
        })
        .collect()
}

/// The items of a Bitwarden JSON export; `Err` for an encrypted export or a file that is not one.
pub fn import_bitwarden_json(text: &str, now: u64) -> Result<Imported, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value: Value = serde_json::from_str(text).map_err(|e| {
        format!(
            "This is not a JSON file (line {}, column {}).",
            e.line(),
            e.column()
        )
    })?;
    let Some(root) = value.as_object() else {
        return Err("This JSON file is not a Bitwarden export.".to_string());
    };
    if root.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(
            "This is an encrypted Bitwarden export. Export the vault again choosing \
                    \"JSON\" (unencrypted), import that file, then delete it."
                .to_string(),
        );
    }
    let Some(entries) = root.get("items").and_then(Value::as_array) else {
        return Err("This JSON file has no \"items\": it is not a Bitwarden export.".to_string());
    };
    let folders: Vec<(String, String)> = root
        .get("folders")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|f| (text_of(f, "id"), text_of(f, "name")))
                .collect()
        })
        .unwrap_or_default();
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for (n, entry) in entries.iter().enumerate() {
        let kind = match entry.get("type").and_then(Value::as_u64) {
            Some(1) => Kind::Login,
            Some(2) => Kind::Note,
            Some(3) => Kind::Card,
            Some(4) => Kind::Identity,
            Some(5) => Kind::SshKey,
            _ => {
                skipped.push(format!("item {}: a type AzKeys does not know", n + 1));
                continue;
            }
        };
        let name = text_of(entry, "name");
        let title = if name.trim().is_empty() {
            "Imported item".to_string()
        } else {
            name
        };
        let mut item = Item::new(kind, &title, now);
        item.notes = text_of(entry, "notes");
        item.favorite = entry
            .get("favorite")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let folder_id = text_of(entry, "folderId");
        if !folder_id.is_empty() {
            if let Some((_, folder)) = folders.iter().find(|(id, _)| *id == folder_id) {
                item.folder = folder.clone();
            }
        }
        if let Some(created) = date_of(entry, "creationDate") {
            item.created = created;
        }
        if let Some(modified) = date_of(entry, "revisionDate") {
            item.modified = modified;
        }
        for field in entry
            .get("fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            // 0 text, 1 hidden, 2 boolean, 3 linked (to a login field: nothing of its own).
            let kind = field.get("type").and_then(Value::as_u64).unwrap_or(0);
            if kind == 3 {
                continue;
            }
            item.fields.push(Field {
                name: text_of(field, "name"),
                value: text_of(field, "value"),
                hidden: kind == 1,
            });
        }
        if let Some(login) = entry.get("login") {
            item.username = text_of(login, "username");
            item.password = text_of(login, "password");
            item.totp = text_of(login, "totp");
            for uri in login
                .get("uris")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let uri = text_of(uri, "uri");
                if !uri.trim().is_empty() {
                    item.urls.push(uri);
                }
            }
            if let Some(changed) = date_of(login, "passwordRevisionDate") {
                item.password_changed = changed;
            }
        }
        let mut history: Vec<PastPassword> = entry
            .get("passwordHistory")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|p| PastPassword {
                password: text_of(p, "password"),
                until: date_of(p, "lastUsedDate").unwrap_or(0),
            })
            .filter(|p| !p.password.is_empty())
            .collect();
        history.sort_by(|a, b| b.until.cmp(&a.until));
        history.truncate(crate::vault::HISTORY_KEPT);
        item.history = history;
        if let Some(card) = entry.get("card") {
            let month = text_of(card, "expMonth");
            let year = text_of(card, "expYear");
            item.card = Card {
                holder: text_of(card, "cardholderName"),
                brand: text_of(card, "brand"),
                number: text_of(card, "number"),
                expiry: match (month.trim(), year.trim()) {
                    ("", "") => String::new(),
                    (m, "") => format!("{m:0>2}"),
                    ("", y) => y.to_string(),
                    (m, y) => format!("{m:0>2}/{y}"),
                },
                code: text_of(card, "code"),
            };
        }
        item.fields
            .extend(fields_from(entry.get("identity"), &IDENTITY_FIELDS));
        item.fields
            .extend(fields_from(entry.get("sshKey"), &SSH_FIELDS));
        items.push(item);
    }
    Ok(Imported {
        format: Format::BitwardenJson,
        items,
        skipped,
    })
}

/// The items of a file the user picked: JSON when it starts with `{`, else CSV.
pub fn import_file(bytes: &[u8], now: u64) -> Result<Imported, String> {
    let bytes = bytes
        .strip_prefix(b"\xef\xbb\xbf".as_slice())
        .unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| {
        "The file is not UTF-8 text: save it as a UTF-8 CSV or as JSON.".to_string()
    })?;
    if text.trim_start().starts_with('{') {
        import_bitwarden_json(text, now)
    } else {
        import_csv(text, now)
    }
}

/// Adds `items` to the vault, leaving out those it already has (same kind, title, user name and
/// password); `(added, already there)`.
pub fn merge(vault: &mut Vault, items: Vec<Item>, now: u64) -> (usize, usize) {
    let (mut added, mut already) = (0, 0);
    for mut item in items {
        let known = vault.items.iter().any(|v| {
            v.kind == item.kind
                && v.title == item.title
                && v.username == item.username
                && v.password == item.password
                && v.notes == item.notes
        });
        if known {
            item.wipe();
            already += 1;
        } else {
            vault.upsert(item, now);
            added += 1;
        }
    }
    (added, already)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Kind;

    const NOW: u64 = 1_790_000_000;

    fn one(text: &str) -> Item {
        let imported = import_csv(text, NOW).expect("a CSV export");
        assert_eq!(imported.items.len(), 1, "{:?}", imported.skipped);
        imported.items.into_iter().next().expect("one")
    }

    #[test]
    fn a_chrome_export_becomes_logins() {
        let text = "name,url,username,password,note\n\
                    code.example.org,https://code.example.org/login,dev@example.org,q7#Rt!vW2m,\"Recovery, see notes\"\n\
                    shop.example,https://shop.example/,me,\"pa\"\"ss\",\n";
        let imported = import_csv(text, NOW).expect("CSV");
        assert_eq!(imported.format, Format::Chrome);
        assert_eq!(imported.items.len(), 2);
        let a = &imported.items[0];
        assert_eq!(a.kind, Kind::Login);
        assert_eq!(a.title, "code.example.org");
        assert_eq!(a.urls, vec!["https://code.example.org/login"]);
        assert_eq!(a.username, "dev@example.org");
        assert_eq!(a.password, "q7#Rt!vW2m");
        assert_eq!(a.notes, "Recovery, see notes");
        assert_eq!(a.created, NOW);
        assert_eq!(imported.items[1].password, "pa\"ss");
        assert!(!a.id.is_empty() && a.id != imported.items[1].id);
    }

    #[test]
    fn a_firefox_export_names_items_by_host_and_keeps_its_dates() {
        let text = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\r\n\
                    \"https://www.forum.example:8443\",\"anna\",\"secret1\",,\"https://www.forum.example\",\"{abc}\",\"1600000000000\",\"1700000000000\",\"1650000000000\"\r\n";
        let item = one(text);
        assert_eq!(item.title, "forum.example");
        assert_eq!(item.created, 1_600_000_000);
        assert_eq!(item.password_changed, 1_650_000_000);
        assert_eq!(import_csv(text, NOW).map(|i| i.format), Ok(Format::Firefox));
    }

    #[test]
    fn a_safari_export_keeps_the_one_time_code() {
        let text = "Title,URL,Username,Password,Notes,OTPAuth\n\
                    Mail (Box example),https://mail.example.org,me@example.org,pw,,otpauth://totp/Mail:me?secret=JBSWY3DPEHPK3PXP&issuer=Mail\n";
        let item = one(text);
        assert_eq!(item.title, "Mail (Box example)");
        assert!(item.totp.starts_with("otpauth://totp/"));
        assert_eq!(import_csv(text, NOW).map(|i| i.format), Ok(Format::Safari));
    }

    #[test]
    fn a_bitwarden_csv_keeps_folder_favourite_type_fields_and_totp() {
        let text = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                    Work,1,login,CodeHost,,\"PIN: 1234\nRecovery: abc\",0,https://code.example.org,dev,pw,JBSWY3DPEHPK3PXP\n\
                    ,,note,Wifi,The password is on the router,,0,,,,\n";
        let imported = import_csv(text, NOW).expect("CSV");
        assert_eq!(imported.format, Format::BitwardenCsv);
        assert_eq!(imported.items.len(), 2);
        let login = &imported.items[0];
        assert_eq!(login.folder, "Work");
        assert!(login.favorite);
        assert_eq!(login.totp, "JBSWY3DPEHPK3PXP");
        let fields: Vec<(&str, &str)> = login
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(fields, vec![("PIN", "1234"), ("Recovery", "abc")]);
        let note = &imported.items[1];
        assert_eq!(note.kind, Kind::Note);
        assert_eq!(note.notes, "The password is on the router");
    }

    #[test]
    fn one_password_keepassxc_and_lastpass_exports_are_read_by_their_headers() {
        let op = "Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n\
                  Forge,https://forge.example,dev,pw,,true,false,\"dev,work\",\n";
        let item = one(op);
        assert_eq!(item.tags, vec!["dev", "work"]);
        assert!(item.favorite);
        assert_eq!(
            import_csv(op, NOW).map(|i| i.format),
            Ok(Format::OnePassword)
        );

        let kp = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\",\"Icon\",\"Last Modified\",\"Created\"\n\
                  \"Root/Internet\",\"Forum\",\"anna\",\"pw\",\"https://forum.example\",\"\",\"\",\"0\",\"2023-01-02T03:04:05Z\",\"2022-01-02T03:04:05Z\"\n";
        let item = one(kp);
        assert_eq!(item.folder, "Root/Internet");
        assert_eq!(item.created, 1_641_092_645);
        assert_eq!(import_csv(kp, NOW).map(|i| i.format), Ok(Format::KeePassXc));

        let lp = "url,username,password,totp,extra,name,grouping,fav\n\
                  http://sn,,,,\"NoteType:Server\nHostname:x\",Server note,Infra,0\n\
                  https://bank.example,anna,pw,,,Bank,Money,1\n";
        let imported = import_csv(lp, NOW).expect("CSV");
        assert_eq!(imported.format, Format::LastPass);
        assert_eq!(
            imported.items[0].kind,
            Kind::Note,
            "LastPass marks notes with the url http://sn"
        );
        assert!(imported.items[0].urls.is_empty());
        assert_eq!(imported.items[1].folder, "Money");
        assert!(imported.items[1].favorite);
    }

    #[test]
    fn rows_with_nothing_to_keep_are_skipped_and_counted_without_their_values() {
        let text = "name,url,username,password\n\
                    ,,,\n\
                    Only a title,,,\n\
                    ,https://x.example,,hunter2\n";
        let imported = import_csv(text, NOW).expect("CSV");
        assert_eq!(imported.items.len(), 2);
        assert_eq!(imported.items[1].title, "x.example");
        assert_eq!(imported.skipped.len(), 1);
        assert!(
            imported.skipped[0].contains("row 2"),
            "{:?}",
            imported.skipped
        );
        assert!(!imported.skipped.join(" ").contains("hunter2"));
    }

    #[test]
    fn a_csv_without_a_column_to_import_or_without_rows_is_refused() {
        assert!(import_csv("", NOW).is_err());
        assert!(import_csv("a,b,c\n1,2,3\n", NOW).is_err());
        assert!(import_csv("\u{feff}name,url,username,password\n", NOW)
            .map(|i| i.items.is_empty())
            .unwrap_or(true));
    }

    #[test]
    fn a_bitwarden_json_export_becomes_logins_notes_cards_identities_and_ssh_keys() {
        let json = r#"{
          "encrypted": false,
          "folders": [{ "id": "f1", "name": "Work" }],
          "items": [
            { "id": "a", "folderId": "f1", "type": 1, "name": "CodeHost", "notes": "n", "favorite": true,
              "fields": [{ "name": "PIN", "value": "1234", "type": 1 }, { "name": "Team", "value": "core", "type": 0 }],
              "login": { "uris": [{ "match": null, "uri": "https://code.example.org" }], "username": "dev",
                         "password": "pw", "totp": "otpauth://totp/CodeHost?secret=JBSWY3DPEHPK3PXP" },
              "passwordHistory": [{ "lastUsedDate": "2024-01-01T00:00:00.000Z", "password": "old" }],
              "creationDate": "2023-05-01T10:00:00.000Z", "revisionDate": "2024-02-01T10:00:00.000Z" },
            { "id": "b", "folderId": null, "type": 2, "name": "Wifi", "notes": "router", "secureNote": { "type": 0 } },
            { "id": "c", "type": 3, "name": "Bank (card)", "card": { "cardholderName": "Anna Berg", "brand": "Visa",
              "number": "4242424242424242", "expMonth": "9", "expYear": "2028", "code": "123" } },
            { "id": "d", "type": 4, "name": "Anna", "identity": { "firstName": "Anna", "lastName": "Berg",
              "email": "anna@example.org", "phone": "+46 1", "city": "Lund", "ssn": null } },
            { "id": "e", "type": 5, "name": "Build box", "sshKey": { "privateKey": "-----BEGIN OPENSSH PRIVATE KEY-----",
              "publicKey": "ssh-ed25519 AAAA", "keyFingerprint": "SHA256:xyz" } }
          ]
        }"#;
        let imported = import_bitwarden_json(json, NOW).expect("a Bitwarden export");
        assert_eq!(imported.format, Format::BitwardenJson);
        let kinds: Vec<Kind> = imported.items.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Kind::Login,
                Kind::Note,
                Kind::Card,
                Kind::Identity,
                Kind::SshKey
            ]
        );

        let login = &imported.items[0];
        assert_eq!(login.folder, "Work");
        assert!(login.favorite);
        assert_eq!(login.urls, vec!["https://code.example.org"]);
        assert_eq!(
            (login.username.as_str(), login.password.as_str()),
            ("dev", "pw")
        );
        assert!(login.totp.starts_with("otpauth://"));
        assert_eq!(login.fields.len(), 2);
        assert!(login.fields[0].hidden && !login.fields[1].hidden);
        assert_eq!(login.history.len(), 1);
        assert_eq!(login.history[0].password, "old");
        assert_eq!(login.history[0].until, 1_704_067_200);
        assert_eq!(login.created, 1_682_935_200);
        assert_eq!(login.modified, 1_706_781_600);

        let card = &imported.items[2];
        assert_eq!(card.card.holder, "Anna Berg");
        assert_eq!(card.card.number, "4242424242424242");
        assert_eq!(card.card.expiry, "09/2028");
        assert_eq!(card.card.code, "123");

        let identity = &imported.items[3];
        let names: Vec<&str> = identity.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(
            names.contains(&"First name") && names.contains(&"Email") && names.contains(&"City")
        );
        assert!(!names.contains(&"SSN"), "an empty value is no field");

        let ssh = &imported.items[4];
        assert!(ssh
            .fields
            .iter()
            .any(|f| f.name == "Private key" && f.hidden));
        assert!(ssh
            .fields
            .iter()
            .any(|f| f.name == "Public key" && !f.hidden));
    }

    #[test]
    fn an_encrypted_bitwarden_export_is_refused_with_what_to_do() {
        let err = import_bitwarden_json(
            r#"{"encrypted": true, "passwordProtected": true, "data": "x"}"#,
            NOW,
        )
        .expect_err("encrypted");
        assert!(err.contains("unencrypted"), "{err}");
        assert!(import_bitwarden_json("[1, 2]", NOW).is_err());
        assert!(import_bitwarden_json("{ broken", NOW).is_err());
    }

    #[test]
    fn a_picked_file_is_read_as_json_or_csv_by_its_content() {
        let json = br#"{"encrypted": false, "items": [{"type": 2, "name": "N", "notes": "x"}]}"#;
        assert_eq!(
            import_file(json, NOW).map(|i| i.format),
            Ok(Format::BitwardenJson)
        );
        let csv = b"\xef\xbb\xbfname,url,username,password\nA,https://a.example,u,p\n";
        assert_eq!(import_file(csv, NOW).map(|i| i.format), Ok(Format::Chrome));
        assert!(import_file(&[0xff, 0xfe, 0x00], NOW).is_err(), "not text");
    }

    #[test]
    fn merging_leaves_out_what_the_vault_already_has() {
        let mut vault = Vault::new("Personal", 1);
        let first =
            import_csv("name,url,username,password\nA,https://a.example,u,p\n", NOW).expect("CSV");
        assert_eq!(merge(&mut vault, first.items, NOW), (1, 0));
        let again = import_csv(
            "name,url,username,password\nA,https://a.example,u,p\nA,https://a.example,u,new\n",
            NOW,
        )
        .expect("CSV");
        assert_eq!(merge(&mut vault, again.items, NOW), (1, 1));
        assert_eq!(vault.items.len(), 2);
    }
}
