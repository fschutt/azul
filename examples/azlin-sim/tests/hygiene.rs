//! X10: the harness's own rules H1 to H10 (planning/architecture.md section 1.4 rules 1 to 4 and 11; the
//! code of `src/` is checked as if it were production code). A source scan.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let src = root().join("src");
    let mut folders = vec![src.clone()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("src is readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path
                    .strip_prefix(&src)
                    .expect("under src")
                    .to_string_lossy()
                    .to_string();
                out.push((name, fs::read_to_string(&path).expect("a source")));
            }
        }
    }
    out.sort();
    out
}

/// The code of a file without its comments and its string literals' contents, line by line.
fn code_lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        let mut code = String::new();
        let mut in_string = false;
        let mut previous = ' ';
        for c in line.chars() {
            if c == '"' && previous != '\\' {
                in_string = !in_string;
                code.push('"');
            } else if !in_string {
                code.push(c);
            }
            previous = c;
        }
        if let Some(at) = code.find("//") {
            code.truncate(at);
        }
        out.push((index + 1, code));
    }
    out
}

#[test]
fn x10_h1_determinism_no_hash_map_no_clock_no_thread_no_environment() {
    let forbidden = [
        "HashMap",
        "HashSet",
        "RandomState",
        "std::time",
        "SystemTime",
        "Instant",
        "thread::",
        "std::env",
        "std::fs",
        "std::process",
        "rand::",
        "f32",
        "f64",
    ];
    let mut hits = Vec::new();
    for (name, text) in sources() {
        if name == "dirstore.rs" || name.starts_with("bin") {
            continue;
        }
        for (line, code) in code_lines(&text) {
            for needle in forbidden {
                if code.contains(needle) {
                    hits.push(format!("src/{name}:{line}: {needle}"));
                }
            }
        }
    }
    assert!(hits.is_empty(), "{}", hits.join("\n"));
}

#[test]
fn x10_h4_no_panics_in_src() {
    let forbidden = [
        ".unwrap()",
        ".expect(",
        "panic!(",
        "unreachable!(",
        "todo!(",
        "unimplemented!(",
    ];
    let mut hits = Vec::new();
    for (name, text) in sources() {
        for (line, code) in code_lines(&text) {
            for needle in forbidden {
                if code.contains(needle) {
                    hits.push(format!("src/{name}:{line}: {needle}"));
                }
            }
        }
    }
    assert!(hits.is_empty(), "{}", hits.join("\n"));
}

#[test]
fn x10_every_src_file_is_declared_and_the_manifest_lists_the_stage_dependencies() {
    let lib = fs::read_to_string(root().join("src").join("lib.rs")).expect("lib.rs");
    for (name, _) in sources() {
        if name == "lib.rs" || name.starts_with("bin") {
            continue;
        }
        let module = name.trim_end_matches(".rs");
        assert!(
            lib.contains(&format!("pub mod {module};")),
            "src/{name} is not declared in lib.rs"
        );
    }
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");
    let dependencies: Vec<&str> = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("a dependencies table")
        .lines()
        .filter(|l| l.contains('='))
        .map(|l| l.split('=').next().unwrap_or("").trim())
        .collect();
    let stage1 = ["azlin-keys", "azlin-seams", "azlin-chain", "azlin-policy"];
    let later = [
        "azlin-teamlog",
        "azlin-sync",
        "azlin-enroll",
        "azlin-links",
        "azlin-regbase",
        "azlin-creds",
        "azlin-merkle",
        "azlin-tlog",
        "azlin-reports",
    ];
    for dep in &dependencies {
        assert!(
            stage1.contains(dep) || later.contains(dep),
            "an unexpected dependency {dep}: nothing third-party"
        );
    }
    for dep in stage1 {
        assert!(
            dependencies.contains(&dep),
            "the stage 1 dependency {dep} is missing"
        );
    }
}

#[test]
fn x10_h5_the_domains_are_test_domains() {
    for domain in azlin_sim::keys::HARNESS_DOMAINS {
        assert!(domain.is_wellformed(), "{domain:?}");
        assert!(
            domain.as_str().starts_with("azlin-sim-"),
            "{domain:?} is not a test domain"
        );
    }
}

#[test]
fn x10_every_public_item_of_the_top_level_cites_a_plan_section_and_a_caller() {
    let mut missing = Vec::new();
    for (name, text) in sources() {
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let is_item = [
                "pub fn ",
                "pub struct ",
                "pub enum ",
                "pub trait ",
                "pub const ",
                "pub type ",
            ]
            .iter()
            .any(|p| line.starts_with(p));
            if !is_item {
                continue;
            }
            let mut doc = String::new();
            let mut at = index;
            while at > 0 {
                at -= 1;
                let above = lines[at].trim_start();
                if above.starts_with("///") || above.starts_with("#[") {
                    doc.push_str(above);
                    doc.push('\n');
                } else {
                    break;
                }
            }
            let cites = doc.contains("planning/architecture.md")
                || doc.contains("plan ")
                || doc.contains("section");
            let caller = doc.contains("Caller:") || doc.contains("Seam:");
            if !cites || !caller {
                missing.push(format!("src/{name}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "public items without a plan section and a Caller: or Seam: line:\n{}",
        missing.join("\n")
    );
}

/// The numbers a text cites as `section N` or `sections N, M`.
fn cited_sections(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in [
        "planning/architecture.md section ",
        "planning/architecture.md sections ",
    ] {
        let mut rest = text;
        while let Some(at) = rest.find(marker) {
            let after = &rest[at + marker.len()..];
            let mut number = String::new();
            for c in after.chars() {
                if c.is_ascii_digit() || c == '.' {
                    number.push(c);
                } else {
                    break;
                }
            }
            let number = number.trim_end_matches('.').to_string();
            if !number.is_empty() {
                out.push(number);
            }
            rest = after;
        }
    }
    out
}

fn plan_path() -> Option<PathBuf> {
    // gate:env allow: the plan is found through AZLIN_PLAN_MD (R7)
    if let Ok(path) = std::env::var("AZLIN_PLAN_MD") {
        return Some(PathBuf::from(path));
    }
    let candidate = root().join("../../../azul-apps/planning/architecture.md");
    candidate.exists().then_some(candidate)
}

#[test]
fn x10_every_plan_section_the_code_cites_exists_in_the_plan() {
    let Some(path) = plan_path() else {
        println!("SKIPPED: plan not present");
        return;
    };
    let plan = fs::read_to_string(Path::new(&path)).expect("the plan is readable");
    let headings: Vec<&str> = plan.lines().filter(|l| l.starts_with('#')).collect();
    let mut cited = Vec::new();
    for (name, text) in sources() {
        for section in cited_sections(&text) {
            cited.push((format!("src/{name}"), section));
        }
    }
    for info in azlin_sim::INVARIANTS {
        for section in info.plan.split(',') {
            let section = section.trim();
            if !section.is_empty() {
                cited.push((info.name.to_string(), section.to_string()));
            }
        }
    }
    let mut missing = Vec::new();
    for (place, section) in cited {
        let found = headings.iter().any(|h| {
            let title = h.trim_start_matches('#').trim();
            title.starts_with(&format!("{section} "))
                || title == section
                || title.starts_with(&format!("{section}."))
        });
        if !found {
            missing.push(format!("{place}: section {section}"));
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "sections cited that are no heading of the plan: {missing:?}"
    );
}

#[test]
fn x10_h8_no_wildcard_arm_over_the_closed_enums() {
    let mut hits = Vec::new();
    for (name, text) in sources() {
        for (line, code) in code_lines(&text) {
            if code.trim_start().starts_with("_ =>") {
                hits.push(format!("src/{name}:{line}"));
            }
        }
    }
    // A wildcard arm is allowed over numbers, options, slices and strings; the closed enums of the brief
    // have none. The files that match over the phase numbers of the jobs, the rounds of the shrinker,
    // the clock shifts and the like are listed by file.
    let allowed = [
        "step.rs",
        "config.rs",
        "seeds.rs",
        "mutants_table.rs",
        "device.rs",
        "leader.rs",
        "invariants.rs",
        "scenario.rs",
        "shrink.rs",
        "time.rs",
        "bin/simrun.rs",
    ];
    let bad: Vec<&String> = hits
        .iter()
        .filter(|h| !allowed.iter().any(|a| h.starts_with(&format!("src/{a}"))))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}
