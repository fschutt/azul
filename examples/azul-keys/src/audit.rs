//! The security audit: logins with a weak password (estimated under 36 bits), a password used by
//! another login too, a password older than two years, a website but no one-time code. Breach
//! checks against online lists are not made (the plan's privacy question).
//!
//! The report written for export names items and problems, never a password.

use std::collections::HashMap;

use crate::generator::{estimate, Strength};
use crate::vault::{Kind, Vault};

/// A password is old after two years.
pub const OLD_SECONDS: u64 = 2 * 365 * 86_400;

/// One thing wrong with a login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Weak,
    /// The password is used by this many logins (2 or more).
    Reused(usize),
    Old,
    NoTwoFactor,
}

impl Problem {
    /// The audit table's words for it.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Problem::Weak => "weak password".to_string(),
            Problem::Reused(n) => format!("reused in {n} items"),
            Problem::Old => "older than 2 years".to_string(),
            Problem::NoTwoFactor => "no one-time code".to_string(),
        }
    }
}

/// The audit's filter (the summary cards).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Weak,
    Reused,
    Old,
    NoTwoFactor,
}

impl Filter {
    pub const ALL: [Filter; 5] = [
        Filter::All,
        Filter::Weak,
        Filter::Reused,
        Filter::Old,
        Filter::NoTwoFactor,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Weak => "Weak",
            Filter::Reused => "Reused",
            Filter::Old => "Old",
            Filter::NoTwoFactor => "No 2FA",
        }
    }

    /// Whether a finding with `problems` shows under the filter.
    #[must_use]
    pub fn holds(self, problems: &[Problem]) -> bool {
        match self {
            Filter::All => true,
            Filter::Weak => problems.contains(&Problem::Weak),
            Filter::Reused => problems.iter().any(|p| matches!(p, Problem::Reused(_))),
            Filter::Old => problems.contains(&Problem::Old),
            Filter::NoTwoFactor => problems.contains(&Problem::NoTwoFactor),
        }
    }
}

/// A login with at least one problem.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// The item's index in the vault.
    pub index: usize,
    pub problems: Vec<Problem>,
    /// The password's estimated strength.
    pub strength: Strength,
    /// When the password was set (or the item changed), seconds since 1970.
    pub changed: u64,
}

/// The audit of a vault.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Audit {
    pub findings: Vec<Finding>,
    pub weak: usize,
    pub reused: usize,
    pub old: usize,
    pub no_two_factor: usize,
}

/// Audits the logins of `vault` at `now`.
#[must_use]
pub fn audit(vault: &Vault, now: u64) -> Audit {
    let audited = |i: usize| {
        let item = &vault.items[i];
        item.kind == Kind::Login && !item.password.is_empty()
    };
    // How many audited logins use each password (compared in memory; nothing is kept).
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for (i, item) in vault.items.iter().enumerate() {
        if audited(i) {
            *uses.entry(item.password.as_str()).or_insert(0) += 1;
        }
    }
    let mut out = Audit::default();
    for (index, item) in vault.items.iter().enumerate() {
        if !audited(index) {
            continue;
        }
        let strength = Strength::of_bits(estimate(&item.password));
        let changed = if item.password_changed > 0 {
            item.password_changed
        } else {
            item.modified.max(item.created)
        };
        let mut problems = Vec::new();
        if strength <= Strength::Weak {
            problems.push(Problem::Weak);
            out.weak += 1;
        }
        let used = uses.get(item.password.as_str()).copied().unwrap_or(1);
        if used > 1 {
            problems.push(Problem::Reused(used));
            out.reused += 1;
        }
        if changed > 0 && now.saturating_sub(changed) > OLD_SECONDS {
            problems.push(Problem::Old);
            out.old += 1;
        }
        if !item.urls.is_empty() && item.totp.trim().is_empty() {
            problems.push(Problem::NoTwoFactor);
            out.no_two_factor += 1;
        }
        if !problems.is_empty() {
            out.findings.push(Finding {
                index,
                problems,
                strength,
                changed,
            });
        }
    }
    out
}

/// The audit as text for export: one line per finding, the item's title and its problems.
#[must_use]
pub fn report(audit: &Audit, vault: &Vault) -> String {
    let mut text = format!(
        "AzKeys security audit of \"{}\"\nweak: {}, reused: {}, older than 2 years: {}, no one-time code: {}\n\n",
        vault.name, audit.weak, audit.reused, audit.old, audit.no_two_factor
    );
    for finding in &audit.findings {
        let Some(item) = vault.items.get(finding.index) else {
            continue;
        };
        let problems: Vec<String> = finding.problems.iter().map(|p| p.label()).collect();
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            item.title,
            problems.join(", "),
            finding.strength.label(),
            if finding.changed > 0 {
                azul_storage::time::iso8601(finding.changed)[..10].to_string()
            } else {
                String::new()
            }
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Item;

    const NOW: u64 = 1_790_000_000;

    fn login(title: &str, password: &str, changed: u64, url: &str, totp: &str) -> Item {
        let mut item = Item::new(Kind::Login, title, changed);
        item.password = password.to_string();
        item.password_changed = changed;
        if !url.is_empty() {
            item.urls.push(url.to_string());
        }
        item.totp = totp.to_string();
        item
    }

    fn vault() -> Vault {
        let mut v = Vault::new("Personal", 1);
        let recent = NOW - 86_400;
        v.items = vec![
            login("Router admin", "admin123", recent, "", ""),
            login(
                "Old forum",
                "Tr0ub4dor&3x!Zq",
                NOW - OLD_SECONDS - 1,
                "https://forum.example",
                "JBSWY3DPEHPK3PXP",
            ),
            login(
                "Shop A",
                "same-Passw0rd-xyz!",
                recent,
                "https://a.example",
                "JBSWY3DPEHPK3PXP",
            ),
            login(
                "Shop B",
                "same-Passw0rd-xyz!",
                recent,
                "https://b.example",
                "JBSWY3DPEHPK3PXP",
            ),
            login(
                "Mail",
                "q7#Rt!vW2mZp8&Lk",
                recent,
                "https://mail.example",
                "",
            ),
            login(
                "Good",
                "u9$Kd!xP3qLm7#Vb",
                recent,
                "https://good.example",
                "JBSWY3DPEHPK3PXP",
            ),
        ];
        let mut card = Item::new(Kind::Card, "Bank (card)", 1);
        card.card.code = "123".to_string();
        v.items.push(card);
        v
    }

    #[test]
    fn weak_reused_old_and_no_2fa_logins_are_found_and_counted() {
        let v = vault();
        let a = audit(&v, NOW);
        assert_eq!((a.weak, a.reused, a.old, a.no_two_factor), (1, 2, 1, 1));
        let titles: Vec<&str> = a
            .findings
            .iter()
            .map(|f| v.items[f.index].title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["Router admin", "Old forum", "Shop A", "Shop B", "Mail"]
        );
        assert_eq!(a.findings[0].problems, vec![Problem::Weak]);
        assert_eq!(a.findings[2].problems, vec![Problem::Reused(2)]);
        assert_eq!(a.findings[4].problems, vec![Problem::NoTwoFactor]);
        let reused: Vec<usize> = a
            .findings
            .iter()
            .filter(|f| Filter::Reused.holds(&f.problems))
            .map(|f| f.index)
            .collect();
        assert_eq!(reused, vec![2, 3]);
    }

    #[test]
    fn cards_notes_and_logins_without_a_password_are_not_audited() {
        let mut v = Vault::new("x", 1);
        v.items.push(login("Empty", "", 1, "https://e.example", ""));
        let mut note = Item::new(Kind::Note, "n", 1);
        note.password = "weak".to_string();
        v.items.push(note);
        assert_eq!(audit(&v, NOW), Audit::default());
    }

    #[test]
    fn the_report_names_items_and_problems_and_no_password() {
        let v = vault();
        let text = report(&audit(&v, NOW), &v);
        assert!(text.contains("Router admin"));
        assert!(text.contains("reused in 2 items"));
        for secret in ["admin123", "same-Passw0rd-xyz!", "Tr0ub4dor", "q7#Rt"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
    }
}
