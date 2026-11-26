//! The generated unit as Free Pascal's object writer and GNU ld meet it.
//!
//! fpc 3.2 (measured with 3.2.2 on Ubuntu 22.04, x86-64, run 45717746672 and the ones before it)
//! gives every routine and every piece of data of a unit a section of its own, and a relocation
//! section beside it. `azul.pas` has 111,742 of them (43,991 `.text.n_*`, 38,386 `.rela.text.n_*`,
//! 14,540 `.data.n_*`, 14,538 `.rela.data.n_*`, for 45,160 routines). An ELF header counts its
//! sections in 16 bits, and the writer built into fpc does not use the extended numbering that is
//! the way out: it writes a header whose count wrapped to 46,208 and whose string table index is
//! out of range, so GNU ld says "azul.o has a corrupt string table index - ignoring", skips the
//! whole object, and the program that uses the unit fails to link with about forty "undefined
//! reference" to symbols the unit defines (`AZUL_$$_AZUL_REFANY_GET`, `VMT_$AZUL_$$_TDOM`,
//! `INIT$_$AZUL`). Compiling the unit reports nothing, which is why the shipped Pascal leg of the
//! language board was the first to see it.
//!
//! Nothing in the source moves it: `{$SMARTLINK OFF}` before or after `unit` leaves the object byte
//! for byte as it was, there is no `{$OUTPUT_FORMAT}` in fpc 3.2, and `-CX- -XX-` on the command
//! line changes nothing (the `fpc.cfg` of Ubuntu smartlinks only on i8086). What works is `-Aas`,
//! GNU `as` writing the object (valid: header count 0 with the real 111,742 in section 0). Taking
//! the value layer's 22,000 forwarders out (44,700 sections) leaves about 67,000, still over, and
//! the rest is wrapper classes that refer to each other and cannot be spread over units. So the unit
//! is not made smaller; every place that tells a Linux user how to compile it says `-Aas`, and these
//! tests fail when the unit is past the limit and one of them does not.

use std::sync::OnceLock;

use super::super::config::CodegenConfig;

/// The unit generated from the real api.json.
fn generated() -> &'static str {
    static OUT: OnceLock<String> = OnceLock::new();
    OUT.get_or_init(|| {
        let ir = crate::codegen::v2::bug_classes::ir();
        super::generate(ir, &CodegenConfig::c_header()).expect("pascal codegen")
    })
}

fn repo_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// What an ELF header can number: section indices from `SHN_LORESERVE` (0xFF00) up are reserved.
const ELF_SECTIONS: usize = 0xFF00;

/// The routines the unit gives a body: the routine headers at the start of a line in its
/// implementation section.
fn routine_bodies(src: &str) -> usize {
    const HEADS: [&str; 6] =
        ["function ", "procedure ", "constructor ", "destructor ", "class function ", "class procedure "];
    let from = src.find("\nimplementation\n").expect("the unit has an implementation section");
    src[from..].lines().filter(|l| HEADS.iter().any(|h| l.starts_with(*h))).count()
}

/// The sections an object of `routines` routines has: 111,742 for 45,160 (2.47 each: a text and a
/// relocation section per routine, a data section and a relocation section per virtual method table
/// and per piece of run-time type information), rounded up to 2.5.
fn object_sections(routines: usize) -> usize {
    routines * 5 / 2
}

/// Is the unit past what the writer of fpc 3.2 can number?
fn past_the_limit() -> bool {
    object_sections(routine_bodies(generated())) >= ELF_SECTIONS
}

#[test]
fn the_estimate_counts_the_routines_of_the_unit() {
    let routines = routine_bodies(generated());
    // 45,160 on 2026-11-25. A unit this API cannot do without is never a tenth of that.
    assert!(routines > 4_000, "only {} routine bodies found: the count does not read the unit", routines);
    assert_eq!(object_sections(45_160), 112_900, "2.5 sections per routine");
}

#[test]
fn the_unit_header_tells_a_debian_user_to_compile_with_aas() {
    if !past_the_limit() {
        return;
    }
    let src = generated();
    let header = &src[..src.find("\nunit Azul;").expect("the unit has a header")];
    assert!(
        header.contains("-Aas"),
        "azul.pas has {} routines, ~{} sections against the {} an ELF header numbers: its header must say \
         to compile with -Aas",
        routine_bodies(src),
        object_sections(routine_bodies(src)),
        ELF_SECTIONS
    );
}

#[test]
fn the_guide_compiles_with_aas_on_linux() {
    if !past_the_limit() {
        return;
    }
    let guide = repo_file("doc/guide/en/hello-world/pascal.md");
    let fpc_lines: Vec<&str> = guide.lines().filter(|l| l.trim_start().starts_with("fpc ")).collect();
    assert!(!fpc_lines.is_empty(), "the guide shows no fpc command");
    assert!(
        fpc_lines.iter().any(|l| l.contains("-Aas")),
        "the unit has more sections than an ELF header numbers, so a Linux command of doc/guide/en/hello-world/pascal.md \
         must carry -Aas; its fpc commands: {:?}",
        fpc_lines
    );
}

#[test]
fn the_e2e_leg_compiles_with_aas_on_linux() {
    if !past_the_limit() {
        return;
    }
    let script = repo_file("scripts/e2e_language_matrix.sh");
    let from = script.find("\nlang_pascal() {").expect("lang_pascal in the language board");
    let leg = &script[from..from + script[from..].find("\n}\n").expect("end of lang_pascal")];
    assert!(
        leg.contains("-Aas"),
        "the unit has more sections than an ELF header numbers, so the Pascal leg of scripts/e2e_language_matrix.sh must \
         compile with -Aas on Linux:\n{}",
        leg
    );
}

#[test]
fn the_linux_install_steps_compile_with_aas() {
    if !past_the_limit() {
        return;
    }
    fn pascal(v: &serde_json::Value) -> Option<&serde_json::Value> {
        match v {
            serde_json::Value::Object(m) => {
                if m.get("displayName").and_then(|d| d.as_str()) == Some("Pascal") && m.contains_key("install") {
                    return Some(v);
                }
                m.values().find_map(pascal)
            }
            serde_json::Value::Array(a) => a.iter().find_map(pascal),
            _ => None,
        }
    }
    let api: serde_json::Value = serde_json::from_str(&repo_file("api.json")).expect("api.json parses");
    let flows = pascal(&api).expect("api.json has the Pascal install steps")["install"].as_array().expect("flows");
    let mut linux = Vec::new();
    for flow in flows {
        let is_linux = flow["os"].as_array().is_some_and(|os| os.iter().any(|o| o == "linux"));
        if !is_linux {
            continue;
        }
        for step in flow["steps"].as_array().into_iter().flatten() {
            if let Some(cmd) = step["content"].as_str().filter(|c| c.starts_with("fpc ")) {
                linux.push(format!("{}: {}", flow["id"].as_str().unwrap_or("?"), cmd));
            }
        }
    }
    assert!(!linux.is_empty(), "api.json has no Linux Pascal flow with an fpc command");
    let without: Vec<&String> = linux.iter().filter(|c| !c.contains("-Aas")).collect();
    assert!(
        without.is_empty(),
        "the unit has more sections than an ELF header numbers, so the Linux install steps of api.json must compile with \
         -Aas: {:?}",
        without
    );
}
