//! Check 2: the `link_policy` row of the account policy (planning/architecture.md section 5.10) and
//! the link options it bounds (section 5.12). Every item the row names has a field of `LinkPolicy`,
//! and every field of `LinkPolicy` is named by the row (`link_policy_covers_every_item_of_the_5_10_row`:
//! the plan is read, and so is the source of the struct); the same for `request_policy`; the option
//! names of the table of 5.12 are the list `LINK_OPTION_NAMES` that `azlin-links` reads too
//! (`link_options_table_of_5_12_parses`), and a copy of the document that adds or drops an option
//! must fail. The type of the audiences is checked on its own.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::str::FromStr;

use azlin_policy::{
    AccountKind, AudienceClass, AudienceSet, Field, LinkPolicy, Policy, RequestPolicy,
    LINK_OPTION_NAMES,
};
use common::{backticked, plan_text, root, table_rows, KINDS};

/// The cell of the row `name` of the table of 5.10.
fn policy_cell(plan: &str, name: &str) -> Result<String, String> {
    let rows = table_rows(
        plan,
        "| Field | personal / family / team / business default |",
    )?;
    for row in rows {
        if let [field, value] = row.as_slice() {
            if backticked(field) == [name] {
                return Ok(value.clone());
            }
        }
    }
    Err(format!("the table of 5.10 has no row {name}"))
}

/// The items the `link_policy` cell names, one text for each: the cell with its ` and ` read as a
/// comma, split at the commas, and the filler words `whether` and `are allowed` taken off.
fn link_items(cell: &str) -> Vec<String> {
    let text = cell
        .trim()
        .trim_end_matches("(5.12)")
        .trim()
        .replace(" and ", ", ");
    text.split(", ")
        .map(|item| {
            let item = item.trim();
            let item = item.strip_prefix("whether ").unwrap_or(item);
            let item = item.strip_suffix(" are allowed").unwrap_or(item);
            item.trim().to_string()
        })
        .collect()
}

/// The items of the `request_policy` cell: split at the commas only (`maximum files and bytes` is
/// one item with two fields).
fn request_items(cell: &str) -> Vec<String> {
    cell.trim()
        .split(", ")
        .map(|item| item.trim().to_string())
        .collect()
}

/// The item of the plan's cell and the field(s) of the struct that carry it.
const LINK_ITEMS: [(&str, &[&str]); 11] = [
    ("the audiences allowed", &["audiences"]),
    ("the maximum expiry", &["max_expiry"]),
    ("a password required for `anyone`", &["password_for_anyone"]),
    ("forced `preview_only`", &["force_preview_only"]),
    ("forced watermark", &["force_watermark"]),
    ("`app_only` forced", &["force_app_only"]),
    ("uploads", &["uploads"]),
    ("edit", &["edit"]),
    ("comments", &["comments"]),
    ("open logs", &["open_logs"]),
    ("slugs", &["slugs"]),
];

const REQUEST_ITEMS: [(&str, &[&str]); 3] = [
    ("allowed", &["allowed"]),
    ("maximum expiry", &["max_expiry"]),
    ("maximum files and bytes", &["max_files", "max_bytes"]),
];

/// The names of the public fields of the struct that starts with `header` in a source file.
fn struct_fields(source: &str, header: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        let t = line.trim();
        if !inside {
            inside = t == header;
            continue;
        }
        if t == "}" {
            return fields;
        }
        if let Some(rest) = t.strip_prefix("pub ") {
            if let Some((name, _)) = rest.split_once(':') {
                fields.push(name.trim().to_string());
            }
        }
    }
    panic!("the struct {header:?} is not in the source");
}

/// Every problem between the two rows of the plan and the two structs.
fn check_policy_rows(plan: &str) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();

    for (row, items, table, header, file, prefix) in [
        (
            "link_policy",
            link_items as fn(&str) -> Vec<String>,
            &LINK_ITEMS[..],
            "pub struct LinkPolicy {",
            "src/link_policy.rs",
            "link_policy.",
        ),
        (
            "request_policy",
            request_items as fn(&str) -> Vec<String>,
            &REQUEST_ITEMS[..],
            "pub struct RequestPolicy {",
            "src/fields.rs",
            "request_policy.",
        ),
    ] {
        let cell = match policy_cell(plan, row) {
            Ok(cell) => cell,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        let items = items(&cell);
        let known: Vec<&str> = table.iter().map(|(item, _)| *item).collect();
        // The document names exactly the items the crate knows, in the same order.
        for item in &items {
            if !known.contains(&item.as_str()) {
                problems.push(format!(
                    "{row}: the plan names an item {item:?} that the crate has no field for"
                ));
            }
        }
        for item in &known {
            if !items.iter().any(|i| i == item) {
                problems.push(format!(
                    "{row}: the crate has a field for {item:?}, which the plan no longer names"
                ));
            }
        }
        // The fields of the struct are exactly the fields of the items.
        let source = fs::read_to_string(root().join(file)).expect("the source file");
        let have: BTreeSet<String> = struct_fields(&source, header).into_iter().collect();
        let want: BTreeSet<String> = table
            .iter()
            .flat_map(|(_, fields)| fields.iter().map(|f| f.to_string()))
            .collect();
        if have != want {
            problems.push(format!(
                "{row}: the fields of the struct are {have:?}, the items of the plan need {want:?}"
            ));
        }
        // And each of them is a leaf of the policy's table of fields.
        let leaves: BTreeSet<String> = Field::ALL
            .iter()
            .filter_map(|f| f.name().strip_prefix(prefix).map(str::to_string))
            .collect();
        if leaves != want {
            problems.push(format!(
                "{row}: the leaves of the field table are {leaves:?}, the items need {want:?}"
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

#[test]
fn link_policy_covers_every_item_of_the_5_10_row() {
    let Some(plan) = plan_text() else {
        return;
    };
    if let Err(problems) = check_policy_rows(&plan) {
        panic!(
            "the policy and the rows of 5.10 differ:\n{}",
            problems.join("\n")
        );
    }
    // The items, as the plan words them (so that the test above reads what it says it reads).
    let cell = policy_cell(&plan, "link_policy").expect("the row");
    assert_eq!(link_items(&cell).len(), 11, "{cell}");
    let cell = policy_cell(&plan, "request_policy").expect("the row");
    assert_eq!(
        request_items(&cell),
        ["allowed", "maximum expiry", "maximum files and bytes"]
    );
}

/// `plan` with `old` replaced by `new`; `old` must be in the document exactly once.
fn edited(plan: &str, old: &str, new: &str) -> String {
    assert_eq!(
        plan.matches(old).count(),
        1,
        "the text to edit is not in the plan exactly once: {old:?}"
    );
    plan.replacen(old, new, 1)
}

#[test]
fn link_policy_test_fails_when_the_row_is_edited() {
    let Some(plan) = plan_text() else {
        return;
    };
    assert_eq!(check_policy_rows(&plan), Ok(()));
    let edits = [
        // An item dropped from the link row.
        ("forced watermark, ", "", "forced watermark"),
        ("`app_only` forced, ", "", "app_only"),
        // An item added to the link row.
        (
            "open logs and slugs are allowed",
            "open logs, tags and slugs are allowed",
            "tags",
        ),
        // The request row loses and gains an item.
        (
            "maximum files and bytes |",
            "maximum files |",
            "maximum files and bytes",
        ),
        (
            "| allowed, maximum expiry,",
            "| allowed, a new bound, maximum expiry,",
            "a new bound",
        ),
    ];
    for (old, new, names) in edits {
        let copy = edited(&plan, old, new);
        match check_policy_rows(&copy) {
            Ok(()) => panic!("the edit {old:?} -> {new:?} was not noticed"),
            Err(problems) => assert!(
                problems.iter().any(|p| p.contains(names)),
                "the edit {old:?} -> {new:?}: the problems do not name {names:?}: {problems:?}"
            ),
        }
    }
}

/// The option names of the table of 5.12, in the order of the table: every backticked name of the
/// option column, except that the two names of a choice (`download` or `preview_only`) are one
/// option and carry the name of their group.
fn option_names(plan: &str) -> Result<Vec<String>, String> {
    let rows = table_rows(plan, "| Group | Option | Values | Strength |")?;
    let mut group = String::new();
    let mut names = Vec::new();
    for row in rows {
        let [first, option, _values, _strength] = row.as_slice() else {
            return Err(format!(
                "a row of the link table has {} cells: {row:?}",
                row.len()
            ));
        };
        if !first.is_empty() {
            group = first.clone();
        }
        let backticks = backticked(option);
        if backticks.is_empty() {
            return Err(format!("a row of the link table names no option: {row:?}"));
        }
        if option.contains("` or `") {
            if group.is_empty() {
                return Err(format!("a choice without a group: {row:?}"));
            }
            names.push(group.replace(' ', "_"));
        } else {
            names.extend(backticks);
        }
    }
    Ok(names)
}

#[test]
fn link_options_table_of_5_12_parses() {
    let Some(plan) = plan_text() else {
        return;
    };
    let names = option_names(&plan).expect("the table of 5.12 parses");
    let listed: Vec<String> = LINK_OPTION_NAMES.iter().map(|n| n.to_string()).collect();
    assert_eq!(
        names, listed,
        "LINK_OPTION_NAMES is the option column of the table of 5.12"
    );
    assert_eq!(names.len(), 24);
    // The names are unique and written the way the plan writes them: lower case with underscores.
    let unique: BTreeSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    for name in &names {
        assert!(
            name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{name} is not lower case with underscores"
        );
    }
}

#[test]
fn link_options_test_fails_when_the_document_adds_or_drops_an_option() {
    let Some(plan) = plan_text() else {
        return;
    };
    let listed: Vec<String> = LINK_OPTION_NAMES.iter().map(|n| n.to_string()).collect();
    assert_eq!(option_names(&plan), Ok(listed.clone()));
    // An option dropped (a row of the table, and one name of a row that names two).
    let copy = edited(
        &plan,
        "| | `idle_expiry` | days without an open | server |\n",
        "",
    );
    assert_ne!(option_names(&copy), Ok(listed.clone()), "a dropped row");
    let copy = edited(&plan, "| `not_before`, `expires_at` |", "| `not_before` |");
    assert_ne!(option_names(&copy), Ok(listed.clone()), "a dropped name");
    // An option added (a row in the table, and a name in a row).
    let copy = edited(
        &plan,
        "| | `slug` |",
        "| | `new_option` | a value | server |\n| | `slug` |",
    );
    assert_ne!(option_names(&copy), Ok(listed.clone()), "an added row");
    let copy = edited(
        &plan,
        "| `title`, `message` |",
        "| `title`, `message`, `tags` |",
    );
    assert_ne!(option_names(&copy), Ok(listed.clone()), "an added name");
    // An option renamed.
    let copy = edited(&plan, "| `subfolders` |", "| `subfolder` |");
    assert_ne!(option_names(&copy), Ok(listed.clone()), "a renamed option");
    // The choice keeps one entry for its two values; splitting it into two options is a change.
    let copy = edited(
        &plan,
        "| `download` or `preview_only` |",
        "| `download`, `preview_only` |",
    );
    assert_ne!(
        option_names(&copy),
        Ok(listed),
        "a choice read as two options"
    );
}

#[test]
fn the_audiences_are_the_five_of_the_table_and_a_set_of_them_is_exact() {
    // The five audiences of 5.12, in the order of the table.
    let words = ["anyone", "signed_in", "members", "people", "approval"];
    assert_eq!(AudienceClass::ALL.len(), 5);
    for (class, word) in AudienceClass::ALL.iter().zip(words) {
        assert_eq!(class.as_str(), word);
        assert_eq!(AudienceClass::from_str(word), Ok(*class));
        assert!(AudienceClass::from_str(&word.to_uppercase()).is_err());
        assert!(AudienceClass::from_str(&format!("{word} ")).is_err());
    }
    assert!(AudienceClass::from_str("approved").is_err());

    // Sets: all, none, with, without, intersection, subset, in the order of the classes.
    let all = AudienceSet::ALL;
    assert_eq!(all.len(), 5);
    assert_eq!(all.classes(), AudienceClass::ALL.to_vec());
    assert!(AudienceSet::NONE.is_empty());
    assert_eq!(AudienceSet::NONE.len(), 0);
    assert_eq!(AudienceSet::NONE.to_string(), "none");
    assert_eq!(all.to_string(), "anyone,signed_in,members,people,approval");
    for class in AudienceClass::ALL {
        assert!(all.contains(*class));
        assert!(!AudienceSet::NONE.contains(*class));
        let only = AudienceSet::NONE.with(*class);
        assert_eq!(only.len(), 1);
        assert!(only.contains(*class));
        assert_eq!(only.without(*class), AudienceSet::NONE);
        assert_eq!(all.without(*class).len(), 4);
        assert!(only.is_subset_of(all));
        assert!(AudienceSet::NONE.is_subset_of(only));
        assert!(!all.is_subset_of(only));
        // Adding twice and removing what is not there change nothing.
        assert_eq!(only.with(*class), only);
        assert_eq!(AudienceSet::NONE.without(*class), AudienceSet::NONE);
    }
    let a = AudienceSet::NONE
        .with(AudienceClass::Anyone)
        .with(AudienceClass::People);
    let b = AudienceSet::NONE
        .with(AudienceClass::People)
        .with(AudienceClass::Members);
    assert_eq!(
        a.intersection(b),
        AudienceSet::NONE.with(AudienceClass::People)
    );
    assert_eq!(a.intersection(b), b.intersection(a));
    assert_eq!(a.intersection(AudienceSet::NONE), AudienceSet::NONE);
    assert_eq!(a.intersection(all), a);
    assert_eq!(a.to_string(), "anyone,people");
    assert_eq!(format!("{a:?}"), "AudienceSet(anyone,people)");
}

#[test]
fn a_workspace_can_switch_public_links_off() {
    // 5.12: "A workspace can switch public links off for its members": the audiences without
    // `anyone`; and no link at all: the empty set.
    let mut policy = Policy::defaults(AccountKind::Business);
    policy.link_policy.audiences = AudienceSet::ALL.without(AudienceClass::Anyone);
    assert!(!policy.link_policy.audiences.contains(AudienceClass::Anyone));
    assert!(policy.link_policy.audiences.contains(AudienceClass::People));
}

#[test]
fn the_default_link_and_request_policies_bound_nothing() {
    // 5.10 gives no value of `link_policy` or `request_policy` for any kind (drift register).
    for kind in KINDS {
        let policy = Policy::defaults(kind);
        assert_eq!(policy.link_policy, LinkPolicy::allow_all(), "{kind}");
        assert_eq!(policy.request_policy, RequestPolicy::unbounded(), "{kind}");
    }
    let link = LinkPolicy::allow_all();
    assert_eq!(link.audiences, AudienceSet::ALL);
    assert_eq!(link.max_expiry, None);
    assert!(
        !link.password_for_anyone
            && !link.force_preview_only
            && !link.force_watermark
            && !link.force_app_only
    );
    assert!(link.uploads && link.edit && link.comments && link.open_logs && link.slugs);
    let request = RequestPolicy::unbounded();
    assert!(request.allowed);
    assert_eq!(request.max_expiry, None);
    assert_eq!(request.max_files, u32::MAX);
    assert_eq!(request.max_bytes, azlin_seams::ByteCount::MAX);
}
