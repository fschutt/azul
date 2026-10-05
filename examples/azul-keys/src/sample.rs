//! The `--sample` vault (the plan's section 6): "Personal" with logins, cards, secure notes,
//! identities and SSH keys - all names, addresses and secrets fictional - including a few weak,
//! reused and old passwords and logins without a one-time code, so the audit has something to
//! show. Its master password is [`SAMPLE_PASSWORD`]; the unlock screen says so for this vault.

use crate::vault::{Card, Field, Item, Kind, Vault};

/// The sample vault's master password (shown on the unlock screen of the sample vault).
pub const SAMPLE_PASSWORD: &str = "sample";

/// The sample vault's name.
pub const SAMPLE_NAME: &str = "Personal";

/// A day in seconds.
const DAY: u64 = 86_400;

/// The fictional services of the generated logins: `(title, host, user)`.
const SERVICES: [(&str, &str, &str); 40] = [
    ("Forge (example)", "forge.example", "dev"),
    ("Shop (example)", "shop.example", "me@example.org"),
    ("Old forum (example)", "forum.example", "anna_b"),
    ("Router admin", "192.168.1.1", "admin"),
    ("Cloud drive (example)", "drive.example", "me@example.org"),
    ("Music (example)", "music.example", "anna"),
    ("Video (example)", "video.example", "anna"),
    ("News (example)", "news.example", "reader42"),
    ("Airline (example)", "fly.example", "BERG/ANNA"),
    ("Hotel (example)", "stay.example", "me@example.org"),
    ("Library card", "library.example", "100200300"),
    ("Insurance (example)", "insure.example", "A-55120"),
    ("Tax office (example)", "tax.example", "anna.berg"),
    (
        "Phone carrier (example)",
        "mobile.example",
        "+46 70 000 00 00",
    ),
    ("Energy (example)", "power.example", "customer-8812"),
    ("Gym (example)", "gym.example", "anna.berg"),
    ("Pharmacy (example)", "pharmacy.example", "me@example.org"),
    ("Train tickets (example)", "rail.example", "me@example.org"),
    ("Car share (example)", "car.example", "anna"),
    ("Photo prints (example)", "prints.example", "me@example.org"),
    ("Recipes (example)", "cook.example", "annab"),
    ("Chat (example)", "chat.example", "anna"),
    ("Wiki (example)", "wiki.example", "AnnaB"),
    ("CI server (example)", "ci.example", "dev"),
    ("Registry (example)", "registry.example", "dev"),
    ("Domain host (example)", "dns.example", "dev@example.org"),
    ("Status page (example)", "status.example", "dev@example.org"),
    ("Analytics (example)", "stats.example", "dev@example.org"),
    ("Design tool (example)", "design.example", "anna"),
    ("Tickets (example)", "events.example", "me@example.org"),
    ("Bookshop (example)", "books.example", "me@example.org"),
    ("Language course (example)", "learn.example", "anna"),
    ("Charity (example)", "give.example", "me@example.org"),
    ("School portal (example)", "school.example", "parent-anna"),
    ("Doctor (example)", "clinic.example", "19850101-0000"),
    ("Parking (example)", "park.example", "ABC123"),
    ("Printer admin", "printer.local", "admin"),
    ("NAS admin", "nas.local", "admin"),
    ("Game store (example)", "games.example", "anna_plays"),
    ("Second mail (example)", "mail2.example", "anna.berg"),
];

/// A deterministic password of `n` characters for sample item `i` (fictional, varied).
fn password_for(i: usize, n: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!#$%&*+-=?@";
    let mut state = (i as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(CHARS[(state % CHARS.len() as u64) as usize])
        })
        .collect()
}

fn login(title: &str, host: &str, user: &str, password: &str, changed: u64) -> Item {
    let mut item = Item::new(Kind::Login, title, changed);
    item.username = user.to_string();
    item.password = password.to_string();
    item.urls.push(format!("https://{host}/"));
    item.password_changed = changed;
    item
}

fn field(name: &str, value: &str, hidden: bool) -> Field {
    Field {
        name: name.to_string(),
        value: value.to_string(),
        hidden,
    }
}

/// The sample vault at `now` (seconds since 1970).
#[must_use]
pub fn sample_vault(now: u64) -> Vault {
    let mut vault = Vault::new(SAMPLE_NAME, now);
    let items = &mut vault.items;

    // The plan's named items.
    let mut codehost = login(
        "CodeHost (example)",
        "code.example.org",
        "dev@example.org",
        "q7#Rt!vW2mZp8&Lk",
        now - 52 * DAY,
    );
    // The RFC 6238 test secret ("12345678901234567890"), so the code can be checked by hand.
    codehost.totp =
        "otpauth://totp/CodeHost%20(example):dev%40example.org?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=CodeHost%20(example)"
            .to_string();
    codehost.favorite = true;
    codehost.tags = vec!["dev".to_string(), "work".to_string()];
    codehost.notes = "Recovery codes in Secure notes / CodeHost.".to_string();
    codehost.history.push(crate::vault::PastPassword {
        password: "codehost-2023".to_string(),
        until: now - 52 * DAY,
    });
    items.push(codehost);

    let mut mail = login(
        "Mail (Box example)",
        "mail.example.org",
        "me@example.org",
        "Lk8&pZm2Wv!tR#7q",
        now - 400 * DAY,
    );
    mail.favorite = true;
    mail.tags = vec!["home".to_string()];
    items.push(mail);

    let mut bank = Item::new(Kind::Card, "Bank (card)", now - 700 * DAY);
    bank.favorite = true;
    bank.card = Card {
        holder: "Anna Berg".to_string(),
        brand: "Visa".to_string(),
        number: "4242 4242 4242 4242".to_string(),
        expiry: "09/2028".to_string(),
        code: "123".to_string(),
    };
    bank.tags = vec!["home".to_string()];
    items.push(bank);

    let mut travel_card = Item::new(Kind::Card, "Travel card (example)", now - 90 * DAY);
    travel_card.card = Card {
        holder: "Anna Berg".to_string(),
        brand: "Mastercard".to_string(),
        number: "5555 5555 5555 4444".to_string(),
        expiry: "03/2027".to_string(),
        code: "321".to_string(),
    };
    items.push(travel_card);

    let mut anna = Item::new(Kind::Identity, "Anna Berg", now - 300 * DAY);
    anna.fields = vec![
        field("First name", "Anna", false),
        field("Last name", "Berg", false),
        field("Email", "anna@example.org", false),
        field("Phone", "+46 70 000 00 00", false),
        field("Address", "Exempelgatan 1", false),
        field("City", "Lund", false),
        field("Country", "Sweden", false),
        field("Passport number", "XX0000000", true),
    ];
    items.push(anna);

    let mut jonas = Item::new(Kind::Identity, "Jonas Lind (work)", now - 120 * DAY);
    jonas.fields = vec![
        field("First name", "Jonas", false),
        field("Last name", "Lind", false),
        field("Company", "Example AB", false),
        field("Email", "jonas@example.org", false),
    ];
    items.push(jonas);

    for (n, host) in ["build-1", "build-2", "laptop", "backup-box"]
        .iter()
        .enumerate()
    {
        let mut key = Item::new(
            Kind::SshKey,
            &format!("SSH {host}"),
            now - (30 + n as u64 * 40) * DAY,
        );
        key.fields = vec![
            field(
                "Private key",
                "-----BEGIN OPENSSH PRIVATE KEY----- (sample, not a real key)",
                true,
            ),
            field(
                "Public key",
                &format!("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5SAMPLE{n} anna@{host}"),
                false,
            ),
            field("Fingerprint", &format!("SHA256:sample{n}"), false),
        ];
        key.tags = vec!["dev".to_string()];
        items.push(key);
    }

    let notes = [
        (
            "CodeHost recovery codes",
            "1111-2222\n3333-4444\n5555-6666 (sample codes)",
        ),
        (
            "Wifi at home",
            "Network: example-home\nPassword: on the router's label",
        ),
        ("Safe combination", "Left 10, right 20, left 30 (sample)"),
        ("Insurance numbers", "Home: H-0000\nCar: C-0000"),
        ("Bike lock", "4 digits: 0000 (sample)"),
    ];
    for (n, (title, text)) in notes.iter().enumerate() {
        let mut note = Item::new(Kind::Note, title, now - (10 + n as u64 * 33) * DAY);
        note.notes = (*text).to_string();
        items.push(note);
    }

    // The generated logins: strong unique passwords mostly; some weak, some reused, some old,
    // most with a website and no one-time code.
    for (i, (title, host, user)) in SERVICES.iter().enumerate() {
        let age_days = 20 + (i as u64 * 47) % 1100;
        let password = match i {
            3 | 36 | 37 => "admin123".to_string(),     // weak and reused
            1 | 18 | 30 => "Summer-2023!".to_string(), // reused
            2 => "anna1985".to_string(),               // weak
            _ => password_for(i, 14 + i % 10),
        };
        let mut item = login(title, host, user, &password, now - age_days * DAY);
        if i % 4 == 0 {
            item.totp = format!(
                "otpauth://totp/{}?secret=JBSWY3DPEHPK3PXP&issuer=Example",
                host.replace('.', "-")
            );
        }
        if i % 7 == 0 {
            item.tags.push("work".to_string());
        }
        if i % 9 == 0 {
            item.tags.push("home".to_string());
        }
        if i == 5 {
            item.favorite = true;
        }
        if i == 2 {
            item.notes = "The old forum; consider deleting the account.".to_string();
        }
        if i == 0 {
            item.fields
                .push(field("Recovery email", "backup@example.org", false));
            item.fields.push(field("PIN", "4711", true));
        }
        items.push(item);
    }
    vault
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::audit;
    use crate::totp::Totp;

    const NOW: u64 = 1_790_000_000;

    #[test]
    fn the_sample_vault_has_the_plans_items_of_every_kind() {
        let v = sample_vault(NOW);
        assert_eq!(v.name, SAMPLE_NAME);
        assert!(v.items.len() >= 60, "{}", v.items.len());
        let counts = v.kind_counts();
        assert!(counts.iter().all(|n| *n > 0), "{counts:?}");
        let titles: Vec<&str> = v.items.iter().map(|i| i.title.as_str()).collect();
        for title in [
            "CodeHost (example)",
            "Mail (Box example)",
            "Bank (card)",
            "Anna Berg",
        ] {
            assert!(titles.contains(&title), "{title}");
        }
        let ids: std::collections::BTreeSet<&str> = v.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids.len(), v.items.len(), "every item has its own id");
    }

    #[test]
    fn every_one_time_code_of_the_sample_parses_and_codehost_gives_the_rfc_code() {
        let v = sample_vault(NOW);
        for item in v.items.iter().filter(|i| !i.totp.is_empty()) {
            assert!(Totp::parse(&item.totp).is_ok(), "{}", item.title);
        }
        let codehost = v
            .items
            .iter()
            .find(|i| i.title == "CodeHost (example)")
            .expect("there");
        assert_eq!(
            Totp::parse(&codehost.totp).expect("parses").code_at(59),
            "287082"
        );
    }

    #[test]
    fn the_sample_gives_the_audit_something_of_each_kind() {
        let v = sample_vault(NOW);
        let a = audit(&v, NOW);
        assert!(
            a.weak >= 2 && a.reused >= 6 && a.old >= 1 && a.no_two_factor >= 10,
            "{a:?}"
        );
    }
}
