//! Check 1: the doc tables. The strength cell of every row of the options table of
//! planning/architecture.md section 5.12 is compared with `STRENGTH`; the values of `audience`, `kind`,
//! `rights` and `notify` with this crate's enums; the names with `azlin_policy::LINK_OPTION_NAMES`;
//! and the whole of it with the contract table of S5 (`azlin-contract`'s `link_options.json`). A copy
//! of the document with one cell edited, one row removed, doubled or renamed must FAIL: every such
//! mutation is tried on the plan (when it is there) and on a copy of the table kept in this file
//! (always).
//!
//! The plan is found by the rule R7 (`AZLIN_PLAN_MD`; else the sibling checkout of azul-apps; when it
//! is absent the tests that read it print `SKIPPED: plan not present` and pass only while
//! `AZLIN_PLAN_MD` is unset).
//!
//! Implements planning/architecture.md section 5.12 (the options table).

mod common;

use std::collections::BTreeSet;
use std::fs;

use azlin_links::{Audience, Group, LinkKind, Notify, OptionName, Rights, Strength, STRENGTH};
use azlin_policy::{AudienceClass, LINK_OPTION_NAMES};
use common::{backticked, every_audience, plan_text, root, split_row, table_rows};

/// The header row of the options table.
const HEADER: &str = "| Group | Option | Values | Strength |";

/// A copy of the options table of 5.12 (planning/architecture.md at the time of writing), so that
/// the checkers' own tests run without the plan. The tests that read the plan are the ones that
/// notice when the plan's table changes.
const TABLE_FIXTURE: &str = r#"| Group | Option | Values | Strength |
|---|---|---|---|
| who | `audience` | `anyone` with the link; `signed_in` (any Azlin account); `members` (this account, or chosen groups); `people` (named people: the share key is wrapped to each one's member key, so a forwarded URL opens nothing); `approval` (anyone may ask, and a device of the sharer approves) | key for `anyone`, `people` and `approval`; server for `signed_in` and `members` |
| | `password` | with any audience; checked with OPAQUE (below) | key and server |
| | `app_only` | the link does not open in `drive.azlin.io` | app |
| what | `kind` | `file`, `selection` (hand-picked files), `folder_snapshot`, `folder_live` (follows edits), `version` (one pinned version) | key: the manifest holds keys only for what is shared, and no other file's name |
| | `subfolders` | with or without | key |
| when | `not_before`, `expires_at` | dates | server |
| | `expires_after_open` | a duration from the first open | server |
| | `idle_expiry` | days without an open | server |
| | `one_time` | the first complete download revokes it | server |
| how much | `max_downloads`, `bytes_cap` | counts, bytes | server |
| | `max_devices` | distinct devices or browsers, counted by a random token each keeps (never an address) | server |
| rights | `download` or `preview_only` | preview only: no save, export, print or copy; screenshots blocked where the platform allows (Android) | app |
| | `allow_upload` | recipients add files through a paired request (Requests, below); the space's devices adopt them into the shared folder | key and server |
| | `edit` | named audiences only: the recipient becomes an external member with write access (a `reg_grant`) once a device of the sharer confirms | key and server |
| | `comment` | signed-in audiences: recipients append to the link's comment log (an append log, 5.13) | server |
| | `allow_reshare` | whether the apps offer to share it on | app |
| | `watermark` | previews carry the viewer's account name, or "link `<id>` · date" for an anonymous viewer | app |
| telling the sharer | `notify` | never, the first open, every open; delivered as `link_used` (5.13) | server |
| | `open_log` | signed-in audiences only, off by default: which account or member opened it and when; the recipient is told before opening (M7) | server |
| presentation | `title`, `message` | sealed under the link key, so only recipients read them | key |
| | `slug` | a readable name instead of the random id; only with a password or a named audience, because a name can be guessed | server |
"#;

/// One row of the table of the plan.
struct PlanRow {
    /// The group (a row with an empty group cell continues the group above).
    group: String,
    /// The backticked names of the option cell, in order.
    listed: Vec<String>,
    /// The option names the row stands for: the listed names, or for the choice "`download` or
    /// `preview_only`" the one option named by its group, `rights`.
    names: Vec<String>,
    /// Whether the row is a choice.
    choice: bool,
    /// The text of the values cell.
    values: String,
    /// The strengths the strength cell names.
    strengths: BTreeSet<Strength>,
}

/// The strengths a strength cell names: in each clause (split at `;`) the leading run of strength
/// words joined by `and` ("key and server"; "key for `anyone`, ...; server for `signed_in` ...";
/// "key: the manifest ...").
fn parse_strengths(cell: &str) -> Result<BTreeSet<Strength>, String> {
    let mut found = BTreeSet::new();
    for clause in cell.split(';') {
        let mut want_strength = true;
        for word in clause.split_whitespace() {
            let bare = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if want_strength {
                match bare.parse::<Strength>() {
                    Ok(strength) => {
                        found.insert(strength);
                        want_strength = false;
                    }
                    Err(_) => break,
                }
            } else if bare == "and" {
                want_strength = true;
            } else {
                break;
            }
        }
    }
    if found.is_empty() {
        return Err(format!("no strength in the cell {cell:?}"));
    }
    Ok(found)
}

/// The rows of the options table of `plan`.
fn plan_rows(plan: &str) -> Result<Vec<PlanRow>, String> {
    let rows = table_rows(plan, HEADER)?;
    let mut group = String::new();
    let mut out = Vec::new();
    for cells in rows {
        let [row_group, option, values, strength] = cells.as_slice() else {
            return Err(format!("a row has {} cells: {cells:?}", cells.len()));
        };
        if !row_group.is_empty() {
            group = row_group.clone();
        }
        if group.is_empty() {
            return Err("a row comes before any group".to_string());
        }
        let listed = backticked(option);
        let choice = option.contains(" or ");
        let names = if choice {
            vec![group.clone()]
        } else {
            listed.clone()
        };
        out.push(PlanRow {
            group: group.clone(),
            listed,
            names,
            choice,
            values: values.clone(),
            strengths: parse_strengths(strength)?,
        });
    }
    Ok(out)
}

/// Every difference between the strengths, names and groups of the plan's table and `STRENGTH`.
fn check_strengths(plan: &str) -> Result<(), Vec<String>> {
    let rows = plan_rows(plan).map_err(|error| vec![error])?;
    let mut problems = Vec::new();
    let mut flat: Vec<(String, String, BTreeSet<Strength>)> = Vec::new();
    for row in &rows {
        for name in &row.names {
            flat.push((name.clone(), row.group.clone(), row.strengths.clone()));
        }
        if row.choice {
            let words: Vec<&str> = Rights::ALL.iter().map(|r| r.as_str()).collect();
            if row.listed != words {
                problems.push(format!(
                    "the choice names {:?}, the crate's `Rights` are {words:?}",
                    row.listed
                ));
            }
        }
    }
    if flat.len() != STRENGTH.len() {
        problems.push(format!(
            "the table has {} options, STRENGTH has {}",
            flat.len(),
            STRENGTH.len()
        ));
    }
    let table_names: Vec<&str> = flat.iter().map(|(name, _, _)| name.as_str()).collect();
    if table_names != LINK_OPTION_NAMES {
        problems.push(format!(
            "the table's option names {table_names:?} are not azlin-policy's LINK_OPTION_NAMES"
        ));
    }
    for (i, (name, group, strengths)) in flat.iter().enumerate() {
        match STRENGTH.get(i) {
            None => problems.push(format!("option {name} (number {i}) is not in STRENGTH")),
            Some((have, have_strengths)) => {
                if have.as_str() != name {
                    problems.push(format!(
                        "number {i}: the table has {name:?}, STRENGTH has {:?}",
                        have.as_str()
                    ));
                }
                if have.group().as_str() != group {
                    problems.push(format!(
                        "{name}: the table's group is {group:?}, the crate's {:?}",
                        have.group().as_str()
                    ));
                }
                let have_set: BTreeSet<Strength> = have_strengths.iter().copied().collect();
                if &have_set != strengths {
                    problems.push(format!(
                        "{name}: the table's strength is {strengths:?}, STRENGTH has {have_set:?}"
                    ));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Notes a problem when `found` (what the plan says) is not `expected` (what the crate says).
fn compare(problems: &mut Vec<String>, what: &str, found: Vec<String>, expected: Vec<&str>) {
    if found != expected {
        problems.push(format!(
            "{what}: the table says {found:?}, the crate {expected:?}"
        ));
    }
}

/// Every difference between the values of `audience`, `kind`, `rights` and `notify` in the plan's
/// table and this crate's enums.
fn check_values(plan: &str) -> Result<(), Vec<String>> {
    let rows = plan_rows(plan).map_err(|error| vec![error])?;
    let find = |name: &str| rows.iter().find(|row| row.names.iter().any(|n| n == name));
    let mut problems = Vec::new();

    match find("audience") {
        Some(row) => compare(
            &mut problems,
            "audience",
            backticked(&row.values),
            AudienceClass::ALL.iter().map(|c| c.as_str()).collect(),
        ),
        None => problems.push("the table has no `audience` row".to_string()),
    }
    match find("kind") {
        Some(row) => compare(
            &mut problems,
            "kind",
            backticked(&row.values),
            LinkKind::ALL.iter().map(|k| k.as_str()).collect(),
        ),
        None => problems.push("the table has no `kind` row".to_string()),
    }
    match find("rights") {
        Some(row) => {
            let mut found = row.listed.clone();
            if !row.choice {
                found.push("(not a choice)".to_string());
            }
            compare(
                &mut problems,
                "rights",
                found,
                Rights::ALL.iter().map(|r| r.as_str()).collect(),
            );
        }
        None => problems.push("the table has no `rights` choice".to_string()),
    }
    match find("notify") {
        Some(row) => {
            let before_semicolon = row.values.split(';').next().unwrap_or("");
            let phrases: Vec<String> = before_semicolon
                .split(", ")
                .map(|phrase| phrase.trim().to_string())
                .collect();
            compare(
                &mut problems,
                "notify",
                phrases,
                Notify::ALL.iter().map(|n| n.doc_phrase()).collect(),
            );
        }
        None => problems.push("the table has no `notify` row".to_string()),
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// `text` with line `i` replaced by `line`.
fn with_line(lines: &[String], i: usize, line: &str) -> String {
    let mut copy = lines.to_vec();
    copy[i] = line.to_string();
    copy.join("\n")
}

/// The first and the last-plus-one line of the options table in `text`.
fn table_range(lines: &[String]) -> (usize, usize) {
    let start = lines
        .iter()
        .position(|line| line.starts_with(HEADER))
        .expect("the header of the options table");
    let end = (start..lines.len())
        .find(|i| !lines[*i].starts_with('|'))
        .unwrap_or(lines.len());
    (start, end)
}

/// A strength word different from what the cell says.
fn other_strength(cell: &str) -> String {
    let current = parse_strengths(cell).expect("a strength cell");
    if current == BTreeSet::from([Strength::App]) {
        "key".to_string()
    } else {
        "app".to_string()
    }
}

/// Every mutation of the table in `text` that changes what the strength test reads: the strength
/// cell of each row edited, the row removed, the row doubled, the first option renamed, the group
/// renamed.
fn strength_mutations(text: &str) -> Vec<(String, String)> {
    let lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    let (start, end) = table_range(&lines);
    let mut out = Vec::new();
    for i in start + 2..end {
        let cells = split_row(&lines[i]);
        let label = format!("row {} ({:?})", i - start - 1, backticked(&cells[1]));
        let join = |cells: &[String]| format!("| {} |", cells.join(" | "));

        // 1. The strength cell edited.
        let mut edited = cells.clone();
        let last = edited.len() - 1;
        edited[last] = other_strength(&cells[last]);
        out.push((
            format!("{label}: the strength cell edited"),
            with_line(&lines, i, &join(&edited)),
        ));

        // 2. The row removed.
        let mut removed = lines.clone();
        removed.remove(i);
        out.push((format!("{label}: removed"), removed.join("\n")));

        // 3. The row doubled.
        let mut doubled = lines.clone();
        doubled.insert(i, lines[i].clone());
        out.push((format!("{label}: doubled"), doubled.join("\n")));

        // 4. The first option of the row renamed.
        let first = backticked(&cells[1]).remove(0);
        let mut renamed = cells.clone();
        renamed[1] = cells[1].replacen(&format!("`{first}`"), &format!("`{first}x`"), 1);
        out.push((
            format!("{label}: the option renamed"),
            with_line(&lines, i, &join(&renamed)),
        ));

        // 5. The group renamed (the row that starts a group says it).
        if !cells[0].is_empty() {
            let mut regrouped = cells.clone();
            regrouped[0] = format!("{}x", cells[0]);
            out.push((
                format!("{label}: the group renamed"),
                with_line(&lines, i, &join(&regrouped)),
            ));
        }
    }
    out
}

/// A mutation of one value of the table: in the row of `row_option`, `needle` becomes
/// `replacement`.
fn value_mutation(text: &str, row_option: &str, needle: &str, replacement: &str) -> String {
    let lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    let (start, end) = table_range(&lines);
    let marker = format!("`{row_option}`");
    for i in start + 2..end {
        let cells = split_row(&lines[i]);
        if cells[1].contains(&marker) {
            assert!(
                lines[i].contains(needle),
                "the row of {row_option} has no {needle:?}: {}",
                lines[i]
            );
            return with_line(&lines, i, &lines[i].replacen(needle, replacement, 1));
        }
    }
    panic!("no row for {row_option}");
}

/// The mutations of the values the value test must notice.
fn value_mutations(text: &str) -> Vec<(String, String)> {
    vec![
        (
            "audience: `approval` is no value".to_string(),
            value_mutation(text, "audience", "`approval`", "approval"),
        ),
        (
            "audience: `signed_in` renamed".to_string(),
            value_mutation(text, "audience", "`signed_in`", "`signed-in`"),
        ),
        (
            "audience: `anyone` renamed".to_string(),
            value_mutation(text, "audience", "`anyone`", "`everyone`"),
        ),
        (
            "kind: `folder_live` renamed".to_string(),
            value_mutation(text, "kind", "`folder_live`", "`folder_lives`"),
        ),
        (
            "kind: `version` is no value".to_string(),
            value_mutation(text, "kind", "`version`", "version"),
        ),
        (
            "rights: `preview_only` renamed".to_string(),
            value_mutation(text, "download", "`preview_only`", "`preview-only`"),
        ),
        (
            "notify: `every open` renamed".to_string(),
            value_mutation(text, "notify", "every open", "each open"),
        ),
        (
            "notify: `the first open` renamed".to_string(),
            value_mutation(text, "notify", "the first open", "first open"),
        ),
    ]
}

#[test]
fn option_strengths_match_architecture_5_12_table() {
    // The copy of the table kept here passes (it is the checker's own test).
    if let Err(problems) = check_strengths(TABLE_FIXTURE) {
        panic!("the copy of the table in this file differs from STRENGTH: {problems:#?}");
    }
    // And the plan's table.
    let Some(plan) = plan_text() else {
        return;
    };
    if let Err(problems) = check_strengths(&plan) {
        panic!("the plan's table (5.12) differs from STRENGTH: {problems:#?}");
    }
    // The plan has 21 rows and 25 backticked names: 24 options (the choice counts once).
    let rows = plan_rows(&plan).expect("the table");
    assert_eq!(rows.len(), 21);
    assert_eq!(rows.iter().map(|row| row.names.len()).sum::<usize>(), 24);
}

#[test]
fn strength_test_fails_when_a_cell_is_edited() {
    // Every mutation of the copy of the table is noticed (always run).
    let mutations = strength_mutations(TABLE_FIXTURE);
    // 21 rows, five mutations each (four for the rows that continue a group above).
    assert_eq!(mutations.len(), 21 * 4 + 7);
    for (what, mutated) in &mutations {
        assert!(
            check_strengths(mutated).is_err(),
            "the strength test did not notice: {what}"
        );
    }
    // The same on the plan itself, when it is there.
    let Some(plan) = plan_text() else {
        return;
    };
    let on_plan = strength_mutations(&plan);
    assert_eq!(on_plan.len(), mutations.len());
    for (what, mutated) in &on_plan {
        assert!(
            check_strengths(mutated).is_err(),
            "the strength test did not notice, on the plan: {what}"
        );
    }
}

#[test]
fn audience_values_and_link_kinds_match_the_table() {
    if let Err(problems) = check_values(TABLE_FIXTURE) {
        panic!("the copy of the table in this file differs from the crate: {problems:#?}");
    }
    // Every class of the policy has an audience of this crate, and the other way round.
    let classes: BTreeSet<AudienceClass> = every_audience().iter().map(|a| a.class()).collect();
    let all: BTreeSet<AudienceClass> = AudienceClass::ALL.iter().copied().collect();
    assert_eq!(classes, all);
    assert_eq!(
        every_audience()
            .iter()
            .map(|a| a.class().as_str())
            .collect::<Vec<_>>(),
        ["anyone", "signed_in", "members", "people", "approval"]
    );
    // The mutations of the values are noticed.
    for (what, mutated) in value_mutations(TABLE_FIXTURE) {
        assert!(
            check_values(&mutated).is_err(),
            "the value test did not notice: {what}"
        );
    }

    let Some(plan) = plan_text() else {
        return;
    };
    if let Err(problems) = check_values(&plan) {
        panic!("the plan's table (5.12) differs from the crate: {problems:#?}");
    }
    for (what, mutated) in value_mutations(&plan) {
        assert!(
            check_values(&mutated).is_err(),
            "the value test did not notice, on the plan: {what}"
        );
    }
}

#[test]
fn option_names_equal_azlin_policys_link_option_names() {
    let ours: Vec<&str> = OptionName::ALL.iter().map(|name| name.as_str()).collect();
    assert_eq!(ours, LINK_OPTION_NAMES);
    let strengths: Vec<&str> = STRENGTH.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(strengths, LINK_OPTION_NAMES);
    assert_eq!(LINK_OPTION_NAMES.len(), 24);
    // The same set, no name twice.
    let set: BTreeSet<&str> = LINK_OPTION_NAMES.iter().copied().collect();
    assert_eq!(set.len(), 24);
    for name in LINK_OPTION_NAMES {
        assert_eq!(name.parse::<OptionName>().map(|n| n.as_str()), Ok(name));
    }
    // The seven groups of the table.
    let groups: Vec<&str> = Group::ALL.iter().map(|g| g.as_str()).collect();
    assert_eq!(
        groups,
        [
            "who",
            "what",
            "when",
            "how much",
            "rights",
            "telling the sharer",
            "presentation"
        ]
    );
}

/// The rows of the table `name` of the contract's fixture: every line that is a JSON array of
/// strings, as the strings.
fn fixture_table(text: &str, name: &str) -> Vec<Vec<String>> {
    let mut inside = false;
    let mut rows = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("\"name\":") {
            inside = t.contains(&format!("\"{name}\""));
            continue;
        }
        if inside && t.starts_with("[\"") {
            rows.push(
                t.split('"')
                    .enumerate()
                    .filter(|(i, _)| i % 2 == 1)
                    .map(|(_, part)| part.to_string())
                    .collect(),
            );
        }
    }
    rows
}

#[test]
fn the_strengths_agree_with_the_contract_table_of_s5() {
    // `azlin-contract` (S5) keeps the plan's table as data and a fixture; the two tables are one
    // table. The fixture is in the same repository.
    let path = root().join("../azlin-contract/fixtures/c4/link_options.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the contract's fixture {} cannot be read: {e}",
            path.display()
        )
    });

    // The options: group, options, "each" or "choice", strengths joined by " + ".
    let mut flat: Vec<(String, String, BTreeSet<Strength>)> = Vec::new();
    let rows = fixture_table(&text, "options");
    assert_eq!(rows.len(), 21);
    for row in &rows {
        let [group, options, kind, strength] = row.as_slice() else {
            panic!("a row of the options table: {row:?}");
        };
        let names: Vec<String> = if kind == "choice" {
            vec![group.clone()]
        } else {
            options.split(", ").map(|name| name.to_string()).collect()
        };
        let strengths: BTreeSet<Strength> = strength
            .split(" + ")
            .map(|word| word.parse::<Strength>().expect("a strength word"))
            .collect();
        for name in names {
            flat.push((name, group.clone(), strengths.clone()));
        }
    }
    assert_eq!(flat.len(), STRENGTH.len());
    for ((name, group, strengths), (ours, ours_strengths)) in flat.iter().zip(STRENGTH.iter()) {
        assert_eq!(name, ours.as_str());
        assert_eq!(group, ours.group().as_str(), "{name}");
        let ours_set: BTreeSet<Strength> = ours_strengths.iter().copied().collect();
        assert_eq!(strengths, &ours_set, "{name}");
    }

    // The strength of each audience.
    let by_audience = fixture_table(&text, "audience_strengths");
    assert_eq!(by_audience.len(), 5);
    for row in &by_audience {
        let [audience, strength] = row.as_slice() else {
            panic!("a row of the audience table: {row:?}");
        };
        let ours: Vec<Audience> = every_audience()
            .into_iter()
            .filter(|a| a.class().as_str() == audience)
            .collect();
        assert_eq!(ours.len(), 1, "{audience}");
        assert_eq!(ours[0].strength().as_str(), strength, "{audience}");
    }

    // The values of the four enumerated options.
    let values = fixture_table(&text, "values");
    let value_of = |option: &str| -> Vec<String> {
        let row = values
            .iter()
            .find(|row| row.first().map(String::as_str) == Some(option))
            .unwrap_or_else(|| panic!("no values for {option}"));
        row[1].split(", ").map(|v| v.to_string()).collect()
    };
    let words = |all: Vec<&str>| -> Vec<String> { all.into_iter().map(String::from).collect() };
    assert_eq!(
        value_of("audience"),
        words(AudienceClass::ALL.iter().map(|c| c.as_str()).collect())
    );
    assert_eq!(
        value_of("kind"),
        words(LinkKind::ALL.iter().map(|k| k.as_str()).collect())
    );
    assert_eq!(
        value_of("rights"),
        words(Rights::ALL.iter().map(|r| r.as_str()).collect())
    );
    assert_eq!(
        value_of("notify"),
        words(Notify::ALL.iter().map(|n| n.as_str()).collect())
    );
}
