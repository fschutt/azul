//! The vault's content, in memory and as the JSON sealed in the vault file (`crypto.rs` seals
//! it): items of five kinds - logins, cards, secure notes, identities, SSH keys - each with a
//! title, the login fields, a one-time-code secret, websites, notes, tags, a favourite flag, a
//! folder (from an import), custom fields (text or hidden), a card's fields, the dates and the
//! passwords it had before.
//!
//! Secrets are plain `String`s here (the window shows and copies them); they never reach a log:
//! [`Item`]'s and [`Vault`]'s `Debug` print no field value, and [`Vault::wipe`] overwrites every
//! string when the vault locks. The plain JSON exists only as a [`Zeroizing`] buffer between
//! [`Vault::to_json`] and the seal.

use std::fmt;

use azul_pim::search::{fold, Query};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

/// What an item is.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Login,
    Card,
    Note,
    Identity,
    SshKey,
}

impl Kind {
    /// Every kind, in the order the navigation lists them.
    pub const ALL: [Kind; 5] = [
        Kind::Login,
        Kind::Card,
        Kind::Note,
        Kind::Identity,
        Kind::SshKey,
    ];

    /// The navigation's name of the kind's items.
    #[must_use]
    pub fn plural(self) -> &'static str {
        match self {
            Kind::Login => "Logins",
            Kind::Card => "Cards",
            Kind::Note => "Secure notes",
            Kind::Identity => "Identities",
            Kind::SshKey => "SSH keys",
        }
    }

    /// One item of the kind ("New login").
    #[must_use]
    pub fn singular(self) -> &'static str {
        match self {
            Kind::Login => "login",
            Kind::Card => "card",
            Kind::Note => "secure note",
            Kind::Identity => "identity",
            Kind::SshKey => "SSH key",
        }
    }

    /// The Material icon name the window shows for the kind.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Login => "key",
            Kind::Card => "credit_card",
            Kind::Note => "sticky_note_2",
            Kind::Identity => "badge",
            Kind::SshKey => "terminal",
        }
    }

    /// The position in [`Kind::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Kind::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// A custom field: a name and a value, hidden (masked, like a password) or not.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Field {
    pub name: String,
    pub value: String,
    pub hidden: bool,
}

/// A payment card's fields.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Card {
    pub holder: String,
    pub brand: String,
    pub number: String,
    /// `MM/YYYY` (or what the import had).
    pub expiry: String,
    /// The security code (CVV / CVC).
    pub code: String,
}

impl Card {
    /// `**** 4242` - the last four digits of the number, for the list.
    #[must_use]
    pub fn masked_number(&self) -> String {
        let digits: Vec<char> = self.number.chars().filter(char::is_ascii_digit).collect();
        if digits.len() < 4 {
            return String::new();
        }
        let last: String = digits[digits.len() - 4..].iter().collect();
        format!("**** {last}")
    }

    fn is_empty(&self) -> bool {
        self.holder.is_empty()
            && self.brand.is_empty()
            && self.number.is_empty()
            && self.expiry.is_empty()
            && self.code.is_empty()
    }
}

/// A password an item had, and until when (seconds since 1970).
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PastPassword {
    pub password: String,
    pub until: u64,
}

/// How many earlier passwords an item keeps.
pub const HISTORY_KEPT: usize = 10;

/// One entry of the vault.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    /// A UUID (the item's name in the vault; never shown).
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub username: String,
    pub password: String,
    /// The one-time-code secret: an `otpauth://totp/...` URL or a base32 secret ("" = none).
    pub totp: String,
    /// The websites the login is for.
    pub urls: Vec<String>,
    pub notes: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    /// The folder (or group) an import put it in ("" = none).
    pub folder: String,
    pub fields: Vec<Field>,
    pub card: Card,
    /// Seconds since 1970.
    pub created: u64,
    pub modified: u64,
    /// When the password was last set (0 = unknown).
    pub password_changed: u64,
    /// The passwords it had before, newest first (at most [`HISTORY_KEPT`]).
    pub history: Vec<PastPassword>,
}

impl fmt::Debug for Item {
    /// The item's shape - kind, title, which fields are set - and never a secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Item")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("title", &self.title)
            .field(
                "username",
                &if self.username.is_empty() {
                    ""
                } else {
                    "(set)"
                },
            )
            .field(
                "password",
                &if self.password.is_empty() { "" } else { "***" },
            )
            .field("totp", &if self.totp.is_empty() { "" } else { "***" })
            .field("urls", &self.urls.len())
            .field("notes", &if self.notes.is_empty() { "" } else { "***" })
            .field("tags", &self.tags)
            .field("favorite", &self.favorite)
            .field("folder", &self.folder)
            .field("fields", &self.fields.len())
            .field("card", &if self.card.is_empty() { "" } else { "***" })
            .field("history", &self.history.len())
            .finish()
    }
}

impl Item {
    /// A new, empty item of `kind` with a fresh id, dated `now`.
    #[must_use]
    pub fn new(kind: Kind, title: &str, now: u64) -> Item {
        Item {
            id: azul_storage::ids::new_uuid(),
            kind,
            title: title.trim().to_string(),
            created: now,
            modified: now,
            ..Item::default()
        }
    }

    /// The list's second line: the user name, a card's last digits, a note's first line.
    #[must_use]
    pub fn subtitle(&self) -> String {
        match self.kind {
            Kind::Card => {
                let masked = self.card.masked_number();
                match (masked.is_empty(), self.card.expiry.trim().is_empty()) {
                    (false, false) => format!("{masked}, {}", self.card.expiry.trim()),
                    (false, true) => masked,
                    _ => self.card.holder.clone(),
                }
            }
            Kind::Note => first_line(&self.notes),
            _ if !self.username.trim().is_empty() => self.username.trim().to_string(),
            _ => self.host().unwrap_or_default(),
        }
    }

    /// The host of the first website (`code.example.org`), if it has one.
    #[must_use]
    pub fn host(&self) -> Option<String> {
        self.urls.iter().find_map(|u| host_of(u))
    }

    /// Up to two initials of the title, for the avatar.
    #[must_use]
    pub fn initials(&self) -> String {
        azul_pim::initials::initials(&self.title)
    }

    /// The text a search looks in, folded: title, user name, websites, tags, folder, the notes
    /// and the names and visible values of the custom fields. Never a password, a hidden value or
    /// a card number.
    #[must_use]
    pub fn search_text(&self) -> String {
        let mut text = String::new();
        let mut push = |s: &str| {
            if !s.is_empty() {
                text.push_str(s);
                text.push('\n');
            }
        };
        push(&self.title);
        push(&self.username);
        for u in &self.urls {
            push(u);
        }
        for t in &self.tags {
            push(t);
        }
        push(&self.folder);
        push(&self.notes);
        push(&self.card.holder);
        push(&self.card.brand);
        for f in &self.fields {
            push(&f.name);
            if !f.hidden {
                push(&f.value);
            }
        }
        push(self.kind.plural());
        fold(&text)
    }

    /// Sets a new password, keeping the old one in the history (newest first, at most
    /// [`HISTORY_KEPT`]). The same password again changes nothing.
    pub fn set_password(&mut self, password: &str, now: u64) {
        if self.password == password {
            return;
        }
        let old = std::mem::replace(&mut self.password, password.to_string());
        if !old.is_empty() {
            self.history.insert(
                0,
                PastPassword {
                    password: old,
                    until: now,
                },
            );
            while self.history.len() > HISTORY_KEPT {
                if let Some(mut dropped) = self.history.pop() {
                    dropped.password.zeroize();
                }
            }
        }
        self.password_changed = now;
        self.modified = now;
    }

    /// Adds a tag (trimmed, without a leading `#`, once).
    pub fn add_tag(&mut self, tag: &str) {
        let tag = tag.trim().trim_start_matches('#').trim();
        if tag.is_empty() || self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            return;
        }
        self.tags.push(tag.to_string());
    }

    /// Overwrites every string of the item (the vault locks).
    pub fn wipe(&mut self) {
        for s in [
            &mut self.id,
            &mut self.title,
            &mut self.username,
            &mut self.password,
            &mut self.totp,
            &mut self.notes,
            &mut self.folder,
            &mut self.card.holder,
            &mut self.card.brand,
            &mut self.card.number,
            &mut self.card.expiry,
            &mut self.card.code,
        ] {
            s.zeroize();
        }
        for s in self.urls.iter_mut().chain(self.tags.iter_mut()) {
            s.zeroize();
        }
        for f in &mut self.fields {
            f.name.zeroize();
            f.value.zeroize();
        }
        for p in &mut self.history {
            p.password.zeroize();
        }
        self.urls.clear();
        self.tags.clear();
        self.fields.clear();
        self.history.clear();
    }
}

/// The first non-blank line of `text`, at most 80 characters.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    line.chars().take(80).collect()
}

/// The host of a URL or of a bare domain: `https://user@code.example.org:443/x` ->
/// `code.example.org`. `None` when nothing host-like is there.
#[must_use]
pub fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = match host.rfind(':') {
        Some(i) if host[i + 1..].chars().all(|c| c.is_ascii_digit()) => &host[..i],
        _ => host,
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    if host.is_empty()
        || host.contains(char::is_whitespace)
        || (!host.contains('.') && host != "localhost")
    {
        return None;
    }
    Some(host)
}

/// A vault: its id and name and its items.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Vault {
    pub id: String,
    pub name: String,
    pub items: Vec<Item>,
    pub created: u64,
    pub modified: u64,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("items", &self.items.len())
            .finish()
    }
}

impl Vault {
    /// A new, empty vault with a fresh id.
    #[must_use]
    pub fn new(name: &str, now: u64) -> Vault {
        Vault {
            id: azul_storage::ids::new_uuid(),
            name: name.trim().to_string(),
            items: Vec::new(),
            created: now,
            modified: now,
        }
    }

    /// The JSON the vault file seals (in a buffer wiped when dropped).
    #[must_use]
    pub fn to_json(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(serde_json::to_vec(self).unwrap_or_default())
    }

    /// The vault of sealed JSON; `Err` says what is wrong (never quoting the content).
    pub fn from_json(bytes: &[u8]) -> Result<Vault, String> {
        serde_json::from_slice(bytes).map_err(|e| {
            format!(
                "the vault's content is not readable (line {}, column {})",
                e.line(),
                e.column()
            )
        })
    }

    /// The item with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The item with `id`, to change.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// Puts `item` in (replacing the one with its id).
    pub fn upsert(&mut self, item: Item, now: u64) {
        match self.items.iter().position(|i| i.id == item.id) {
            Some(i) => self.items[i] = item,
            None => self.items.push(item),
        }
        self.modified = now;
    }

    /// Takes the item with `id` out (wiped).
    pub fn remove(&mut self, id: &str, now: u64) -> bool {
        let Some(i) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        let mut item = self.items.remove(i);
        item.wipe();
        self.modified = now;
        true
    }

    /// Every tag with the number of its items, by name (case folded).
    #[must_use]
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut tags: Vec<(String, usize)> = Vec::new();
        for item in &self.items {
            for tag in &item.tags {
                match tags.iter_mut().find(|(t, _)| t.eq_ignore_ascii_case(tag)) {
                    Some((_, n)) => *n += 1,
                    None => tags.push((tag.clone(), 1)),
                }
            }
        }
        tags.sort_by_key(|(t, _)| fold(t));
        tags
    }

    /// The number of items of each kind, in [`Kind::ALL`]'s order.
    #[must_use]
    pub fn kind_counts(&self) -> [usize; 5] {
        let mut counts = [0; 5];
        for item in &self.items {
            counts[item.kind.index()] += 1;
        }
        counts
    }

    /// Overwrites every string and empties the vault (it locks).
    pub fn wipe(&mut self) {
        for item in &mut self.items {
            item.wipe();
        }
        self.items.clear();
        self.name.zeroize();
        self.id.zeroize();
    }
}

/// Which items the list shows.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Favorites,
    Kind(Kind),
    Tag(String),
    /// Logins with a one-time code.
    OneTimeCodes,
}

impl Scope {
    /// Whether `item` is in the scope.
    #[must_use]
    pub fn holds(&self, item: &Item) -> bool {
        match self {
            Scope::All => true,
            Scope::Favorites => item.favorite,
            Scope::Kind(k) => item.kind == *k,
            Scope::Tag(t) => item.tags.iter().any(|x| x.eq_ignore_ascii_case(t)),
            Scope::OneTimeCodes => !item.totp.trim().is_empty(),
        }
    }

    /// The list's heading.
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Scope::All => "All items".to_string(),
            Scope::Favorites => "Favourites".to_string(),
            Scope::Kind(k) => k.plural().to_string(),
            Scope::Tag(t) => format!("#{t}"),
            Scope::OneTimeCodes => "One-time codes".to_string(),
        }
    }
}

/// The key the list sorts by: the folded title, then the user name.
fn sort_key(item: &Item) -> (String, String) {
    (fold(item.title.trim()), fold(item.username.trim()))
}

/// The indices of the items in `scope` that match `query` (every word, diacritics folded), by
/// title.
#[must_use]
pub fn view(vault: &Vault, scope: &Scope, query: &str) -> Vec<usize> {
    let query = Query::parse(query);
    let mut indices: Vec<usize> = vault
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| scope.holds(item))
        .filter(|(_, item)| query.is_empty() || query.matches_folded(&item.search_text()))
        .map(|(i, _)| i)
        .collect();
    indices.sort_by_cached_key(|&i| sort_key(&vault.items[i]));
    indices
}

/// The list's letter sections: `(letter, indices)` in the order of `indices` (sorted by title);
/// a title starting with a digit or a sign goes under `#`.
#[must_use]
pub fn sections(vault: &Vault, indices: &[usize]) -> Vec<(char, Vec<usize>)> {
    let mut out: Vec<(char, Vec<usize>)> = Vec::new();
    for &i in indices {
        let letter = fold(vault.items[i].title.trim())
            .chars()
            .next()
            .filter(char::is_ascii_alphabetic)
            .map_or('#', |c| c.to_ascii_uppercase());
        match out.last_mut() {
            Some((l, members)) if *l == letter => members.push(i),
            _ => out.push((letter, vec![i])),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login(title: &str, user: &str, password: &str) -> Item {
        let mut item = Item::new(Kind::Login, title, 100);
        item.username = user.to_string();
        item.password = password.to_string();
        item
    }

    fn sample() -> Vault {
        let mut v = Vault::new("Personal", 100);
        let mut a = login("CodeHost (example)", "dev@example.org", "q7#Rt!vW2m");
        a.urls.push("https://code.example.org/login".to_string());
        a.tags = vec!["dev".to_string(), "work".to_string()];
        a.favorite = true;
        let mut b = login("Müller Bank", "anna", "hunter2");
        b.tags = vec!["Home".to_string()];
        let mut c = Item::new(Kind::Card, "Bank (card)", 100);
        c.card.number = "4242 4242 4242 4242".to_string();
        c.card.expiry = "09/2028".to_string();
        let mut d = Item::new(Kind::Note, "2FA recovery", 100);
        d.notes = "\n  Recovery codes for CodeHost\nabc-def".to_string();
        v.items = vec![a, b, c, d];
        v
    }

    #[test]
    fn a_vault_round_trips_through_its_json() {
        let v = sample();
        let json = v.to_json();
        let back = Vault::from_json(&json).expect("readable");
        assert_eq!(back, v);
        assert!(Vault::from_json(b"{ not json").is_err());
    }

    #[test]
    fn the_debug_form_of_an_item_or_a_vault_shows_no_secret() {
        let v = sample();
        let text = format!("{v:?} {:?} {:?}", v.items[0], v.items[2]);
        for secret in ["q7#Rt!vW2m", "hunter2", "4242 4242", "dev@example.org"] {
            assert!(!text.contains(secret), "{secret} leaked into {text}");
        }
    }

    #[test]
    fn search_finds_title_user_website_tag_and_notes_with_diacritics_folded() {
        let v = sample();
        let titles = |q: &str| -> Vec<String> {
            view(&v, &Scope::All, q)
                .into_iter()
                .map(|i| v.items[i].title.clone())
                .collect()
        };
        assert_eq!(titles("muller"), vec!["Müller Bank"]);
        assert_eq!(titles("code.example"), vec!["CodeHost (example)"]);
        assert_eq!(titles("#work"), vec!["CodeHost (example)"]);
        assert_eq!(titles("recovery codes"), vec!["2FA recovery"]);
        assert!(titles("hunter2").is_empty(), "a password is never searched");
        assert!(titles("4242").is_empty(), "a card number is never searched");
        assert_eq!(titles("").len(), 4);
    }

    #[test]
    fn scopes_filter_by_favourite_kind_tag_and_one_time_code() {
        let mut v = sample();
        v.items[1].totp = "JBSWY3DPEHPK3PXP".to_string();
        assert_eq!(view(&v, &Scope::Favorites, "").len(), 1);
        assert_eq!(view(&v, &Scope::Kind(Kind::Card), "").len(), 1);
        assert_eq!(view(&v, &Scope::Tag("home".to_string()), "").len(), 1);
        assert_eq!(view(&v, &Scope::OneTimeCodes, "").len(), 1);
        assert_eq!(v.kind_counts(), [2, 1, 1, 0, 0]);
        assert_eq!(
            v.tags(),
            vec![
                ("dev".to_string(), 1),
                ("Home".to_string(), 1),
                ("work".to_string(), 1)
            ]
        );
    }

    #[test]
    fn the_list_is_sorted_by_title_in_letter_sections() {
        let v = sample();
        let all = view(&v, &Scope::All, "");
        let titles: Vec<&str> = all.iter().map(|&i| v.items[i].title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "2FA recovery",
                "Bank (card)",
                "CodeHost (example)",
                "Müller Bank"
            ]
        );
        let letters: Vec<char> = sections(&v, &all).into_iter().map(|(l, _)| l).collect();
        assert_eq!(letters, vec!['#', 'B', 'C', 'M']);
    }

    #[test]
    fn subtitles_show_the_user_the_card_digits_or_the_note() {
        let v = sample();
        assert_eq!(v.items[0].subtitle(), "dev@example.org");
        assert_eq!(v.items[2].subtitle(), "**** 4242, 09/2028");
        assert_eq!(v.items[3].subtitle(), "Recovery codes for CodeHost");
        let mut bare = Item::new(Kind::Login, "Router", 1);
        bare.urls.push("http://admin@192.168.1.1:8080/".to_string());
        assert_eq!(bare.subtitle(), "192.168.1.1");
    }

    #[test]
    fn hosts_are_read_from_urls_and_bare_domains() {
        assert_eq!(
            host_of("https://www.Example.org/x?y").as_deref(),
            Some("example.org")
        );
        assert_eq!(
            host_of("code.example.org").as_deref(),
            Some("code.example.org")
        );
        assert_eq!(
            host_of("android://abc@com.example.app/").as_deref(),
            Some("com.example.app")
        );
        assert_eq!(
            host_of("http://localhost:3000").as_deref(),
            Some("localhost")
        );
        assert_eq!(host_of("not a url"), None);
        assert_eq!(host_of("a b.c"), None);
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn a_new_password_keeps_the_old_one_in_the_history() {
        let mut item = login("Shop", "me", "first");
        for (n, p) in ["second", "third"].iter().enumerate() {
            item.set_password(p, 200 + n as u64);
        }
        assert_eq!(item.password, "third");
        let old: Vec<&str> = item.history.iter().map(|p| p.password.as_str()).collect();
        assert_eq!(old, vec!["second", "first"]);
        assert_eq!(item.password_changed, 201);
        item.set_password("third", 300);
        assert_eq!(
            item.history.len(),
            2,
            "the same password again is no change"
        );
        for n in 0..20 {
            item.set_password(&format!("p{n}"), 400 + n);
        }
        assert_eq!(item.history.len(), HISTORY_KEPT);
    }

    #[test]
    fn locking_wipes_every_string_of_the_vault() {
        let mut v = sample();
        v.wipe();
        assert!(v.items.is_empty());
        assert!(v.name.is_empty());
        let mut item = login("A", "b", "c");
        item.fields.push(Field {
            name: "PIN".into(),
            value: "1234".into(),
            hidden: true,
        });
        item.wipe();
        assert!(item.password.is_empty() && item.fields.is_empty() && item.title.is_empty());
    }

    #[test]
    fn tags_are_added_once_without_a_hash() {
        let mut item = login("A", "", "");
        item.add_tag(" #Work ");
        item.add_tag("work");
        item.add_tag("  ");
        assert_eq!(item.tags, vec!["Work"]);
    }
}
