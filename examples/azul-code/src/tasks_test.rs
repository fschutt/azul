//! The tasks a folder's files give: cargo's, a package.json's scripts, a Makefile's targets; which names are plain
//! enough to type into a shell.

use super::*;

fn labels(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.label.as_str()).collect()
}

#[test]
fn a_cargo_package_has_run_build_test_and_check() {
    let tasks = from_cargo("[package]\nname = \"x\"\n");
    assert_eq!(labels(&tasks), ["run", "build", "test", "check"]);
    assert_eq!(tasks[0].command, "cargo run");
    assert!(tasks.iter().all(|t| t.source == Source::Cargo));
}

#[test]
fn a_cargo_workspace_alone_has_nothing_of_its_own_to_run() {
    let tasks = from_cargo("[workspace]\nmembers = [\"a\"]\n");
    assert_eq!(labels(&tasks), ["build", "test", "check"]);
    // a manifest that is neither is no project
    assert!(from_cargo("# nothing\n").is_empty());
    assert!(from_cargo("").is_empty());
}

#[test]
fn a_package_that_is_also_a_workspace_runs() {
    let tasks = from_cargo("[workspace]\nmembers = []\n\n[package]\nname = \"root\"\n");
    assert_eq!(labels(&tasks), ["run", "build", "test", "check"]);
}

#[test]
fn the_scripts_of_a_package_json_are_npm_run_by_name() {
    let json = r#"{"name":"x","scripts":{"start":"node a.js","build":"tsc","test":"jest","lint:fix":"eslint --fix"}}"#;
    let tasks = from_package_json(json);
    assert_eq!(labels(&tasks), ["build", "lint:fix", "start", "test"]);
    assert_eq!(tasks[1].command, "npm run lint:fix");
    assert!(tasks.iter().all(|t| t.source == Source::Npm));
}

#[test]
fn a_package_json_that_says_no_scripts_or_is_no_json_has_no_tasks() {
    assert!(from_package_json(r#"{"name":"x"}"#).is_empty());
    assert!(from_package_json(r#"{"scripts":"not an object"}"#).is_empty());
    assert!(from_package_json("not json at all").is_empty());
    assert!(from_package_json("").is_empty());
}

#[test]
fn a_script_name_a_shell_would_read_as_more_than_one_word_is_left_out() {
    let json = r#"{"scripts":{"ok":"a","x; rm -rf ~":"b","two words":"c","$(id)":"d","a&&b":"e","-rf":"f","fine-name_1.2":"g"}}"#;
    assert_eq!(labels(&from_package_json(json)), ["fine-name_1.2", "ok"]);
}

#[test]
fn the_targets_of_a_makefile_once_each_without_the_pattern_and_the_special_ones() {
    let text = "\
.PHONY: all clean
CC := gcc
CFLAGS ?= -O2

all: main.o util.o
\t$(CC) -o app main.o util.o

%.o: %.c
\t$(CC) -c $<

clean:
\trm -f *.o app

test check: all
\t./app --test

# build: not a target
  indented: not either
all: again
";
    let tasks = from_makefile(text);
    assert_eq!(labels(&tasks), ["all", "clean", "test", "check"]);
    assert_eq!(tasks[0].command, "make all");
    assert!(tasks.iter().all(|t| t.source == Source::Make));
}

#[test]
fn a_makefile_target_with_a_variable_or_odd_characters_is_left_out() {
    let text = "$(OUT): deps\n\tx\nweird;name: y\n\tz\nok: y\n";
    assert_eq!(labels(&from_makefile(text)), ["ok"]);
}

#[test]
fn a_kind_lists_at_most_forty_tasks() {
    let mut makefile = String::new();
    for i in 0..100 {
        makefile.push_str(&format!("t{i}:\n\tx\n"));
    }
    assert_eq!(from_makefile(&makefile).len(), MAX_TASKS);
    let scripts: Vec<String> = (0..100).map(|i| format!("\"s{i}\":\"x\"")).collect();
    let json = format!("{{\"scripts\":{{{}}}}}", scripts.join(","));
    assert_eq!(from_package_json(&json).len(), MAX_TASKS);
}

#[test]
fn the_root_files_of_a_folder_give_their_tasks_cargo_first_then_npm_then_make() {
    let files = vec![
        ("Makefile".to_string(), "all:\n\tx\n".to_string()),
        ("package.json".to_string(), r#"{"scripts":{"dev":"x"}}"#.to_string()),
        ("Cargo.toml".to_string(), "[package]\nname=\"a\"\n".to_string()),
        ("README.md".to_string(), "not a task file".to_string()),
    ];
    let tasks = detect(&files);
    let commands: Vec<&str> = tasks.iter().map(|t| t.command.as_str()).collect();
    assert_eq!(commands, ["cargo run", "cargo build", "cargo test", "cargo check", "npm run dev", "make all"]);
    assert!(detect(&[]).is_empty(), "a folder with none of the files has no tasks");
}

#[test]
fn a_name_is_plain_when_a_shell_reads_it_as_one_word() {
    for ok in ["build", "lint:fix", "a-b_c.d", "@scope/pkg", "x1", "ünï"] {
        assert!(plain_name(ok), "{ok}");
    }
    for bad in ["", "-x", "a b", "a;b", "a|b", "a`b`", "a$b", "a\"b", "a'b", "a\nb", "a>b", "a*b"] {
        assert!(!plain_name(bad), "{bad:?}");
    }
    assert!(!plain_name(&"x".repeat(101)));
}

#[test]
fn every_source_names_its_words_and_its_file() {
    let all = [Source::Cargo, Source::Npm, Source::Make];
    let mut names: Vec<&str> = all.iter().map(|s| s.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), all.len());
    for s in all {
        assert!(!s.file().is_empty(), "{s:?}");
    }
}
