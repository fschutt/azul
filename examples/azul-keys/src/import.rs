//! Import: the CSV files browsers and password managers export, and Bitwarden's JSON export.
//!
//! CSV (read with the `csv` crate: quotes, separators and line breaks in fields, a byte-order
//! mark): the columns are found by their header names, so one reader takes
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

use crate::vault::{Item, Vault};

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

/// The format a CSV header names.
#[must_use]
pub fn detect(headers: &[String]) -> Format {
    let _ = headers;
    todo!("GREEN")
}

/// The items of a CSV export; `Err` when the file is no table or has no column to import.
pub fn import_csv(text: &str, now: u64) -> Result<Imported, String> {
    let _ = (text, now);
    todo!("GREEN")
}

/// The items of a Bitwarden JSON export; `Err` for an encrypted export or a file that is not one.
pub fn import_bitwarden_json(text: &str, now: u64) -> Result<Imported, String> {
    let _ = (text, now);
    todo!("GREEN")
}

/// The items of a file the user picked: JSON when it starts with `{`, else CSV.
pub fn import_file(bytes: &[u8], now: u64) -> Result<Imported, String> {
    let _ = (bytes, now);
    todo!("GREEN")
}

/// Adds `items` to the vault, leaving out those it already has (same kind, title, user name and
/// password); `(added, already there)`.
pub fn merge(vault: &mut Vault, items: Vec<Item>, now: u64) -> (usize, usize) {
    let _ = (vault, items, now);
    todo!("GREEN")
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
