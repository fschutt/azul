//! The whiteboard's two settings: who a published board names as its author, and the template of a link to code for a
//! host of no known form. No window, no azul.

use super::*;

fn parse(pairs: &[(&str, &str)]) -> (Settings, Vec<String>) {
    Settings::parse(&|key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_string()))
}

#[test]
fn nothing_set_is_git_s_name_and_the_github_form() {
    let (s, notes) = parse(&[]);
    assert_eq!(s, Settings { author: Author::Git, permalink_template: None });
    assert!(notes.is_empty());
    assert_eq!(s.author_name(Some("Grace Hopper")).as_deref(), Some("Grace Hopper"));
    assert_eq!(s.author_name(None), None, "git has no name: none is written");
    assert_eq!(s.author_value(), "git");
}

#[test]
fn the_author_is_git_s_name_none_or_a_name_of_the_person_s_own() {
    for git in ["", "  ", "git", "GIT", " Git "] {
        assert_eq!(parse(&[(KEY_AUTHOR, git)]).0.author, Author::Git, "{git:?}");
    }
    for off in ["off", "OFF", " off "] {
        let s = parse(&[(KEY_AUTHOR, off)]).0;
        assert_eq!(s.author, Author::Off);
        assert_eq!(s.author_name(Some("Grace")), None, "none, whatever git says");
        assert_eq!(s.author_value(), "off");
    }
    let s = parse(&[(KEY_AUTHOR, "  The Architecture Team ")]).0;
    assert_eq!(s.author, Author::Name("The Architecture Team".to_string()));
    assert_eq!(s.author_name(Some("Grace")).as_deref(), Some("The Architecture Team"), "the name of the setting, not git's");
    assert_eq!(s.author_value(), "The Architecture Team");
}

#[test]
fn a_name_that_is_no_name_is_told_and_git_s_is_used() {
    let long = "x".repeat(201);
    for bad in ["two\nlines", long.as_str(), "tab\there"] {
        let (s, notes) = parse(&[(KEY_AUTHOR, bad)]);
        assert_eq!(s.author, Author::Git, "{bad:?}");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains(KEY_AUTHOR), "{}", notes[0]);
    }
    let fine = "x".repeat(200);
    assert_eq!(parse(&[(KEY_AUTHOR, fine.as_str())]).0.author, Author::Name(fine.clone()), "200 is allowed");
}

#[test]
fn a_template_for_a_link_needs_a_host_a_revision_and_a_path() {
    let good = "https://git.example.org/{web}/blob/{rev}/{path}#L{start}";
    assert_eq!(parse(&[(KEY_PERMALINK, good)]).0.permalink_template.as_deref(), Some(good));
    let web = "{web}/src/{rev}/{path}#{start}-{end}";
    assert_eq!(parse(&[(KEY_PERMALINK, web)]).0.permalink_template.as_deref(), Some(web));
    assert_eq!(parse(&[(KEY_PERMALINK, "")]).0.permalink_template, None);
    let long = format!("https://x.org/{{rev}}/{{path}}/{}", "a".repeat(500));
    for bad in ["https://x.org/{path}", "https://x.org/{rev}", "ftp://x.org/{rev}/{path}", "javascript:{rev}{path}", "https://x.org/ {rev}/{path}", "{rev}/{path}", long.as_str()] {
        let (s, notes) = parse(&[(KEY_PERMALINK, bad)]);
        assert_eq!(s.permalink_template, None, "{bad:?}");
        assert_eq!(notes.len(), 1, "{bad:?}");
        assert!(notes[0].contains(KEY_PERMALINK));
    }
}
