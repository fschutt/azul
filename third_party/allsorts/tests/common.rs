use std::{
    io::BufRead,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use regex::Regex;

pub fn fixture_path<P: AsRef<Path>>(path: P) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

/// Read a test fixture from a path relative to CARGO_MANIFEST_DIR
pub fn read_fixture<P: AsRef<Path>>(path: P) -> Vec<u8> {
    std::fs::read(&fixture_path(path)).expect("error reading file contents")
}

#[cfg(not(feature = "prince"))]
#[allow(unused)]
pub fn read_fixture_font<P: AsRef<Path>>(path: P) -> Vec<u8> {
    read_fixture(Path::new("tests/fonts").join(path))
}

#[cfg(feature = "prince")]
#[allow(unused)]
pub fn read_fixture_font<P: AsRef<Path>>(path: P) -> Vec<u8> {
    [
        Path::new("tests/fonts").join(path.as_ref()),
        Path::new("../../../tests/data/fonts").join(path.as_ref()),
    ]
    .iter()
    .find(|path| path.is_file())
    .map(read_fixture)
    .unwrap_or_else(|| panic!("unable to find fixture font {}", path.as_ref().display()))
}

#[allow(unused)]
fn parse_expected_output(expected_output: &str, ignore: &[u16]) -> (Vec<u16>, Option<String>) {
    fn parse(s: &str, ignore: &[u16]) -> Vec<u16> {
        s.split('|')
            .map(|s| s.parse::<u16>().expect("error parsing glyph index"))
            .filter(|i| ignore.is_empty() || !ignore.contains(i))
            .collect()
    }

    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX
        .get_or_init(|| Regex::new(r"^\[(\d+(?:\|\d+)*)\](?:\s*:\s*(.*))?$").unwrap());

    if let Some(captures) = regex.captures(expected_output) {
        let indices = parse(&captures[1], ignore);
        let reason = captures.get(2).map(|s| String::from(s.as_str()));

        (indices, reason)
    } else {
        panic!("invalid expected output format: {:?}", expected_output);
    }
}

#[allow(unused)]
pub fn read_inputs<P: AsRef<Path>, B: AsRef<Path>>(base: B, inputs_path: P) -> Vec<String> {
    read_fixture_inputs(base, inputs_path)
        .lines()
        .collect::<Result<_, _>>()
        .expect("error reading inputs")
}

#[allow(unused)]
fn read_fixture_inputs<P: AsRef<Path>, B: AsRef<Path>>(base: B, path: P) -> Vec<u8> {
    read_fixture(base.as_ref().join(path))
}

#[allow(unused)]
pub fn parse_expected_outputs<B: AsRef<Path>, P: AsRef<Path>>(
    base: B,
    expected_outputs_path: P,
    ignore: &[u16],
) -> Vec<(Vec<u16>, Option<String>)> {
    read_fixture_inputs(base, expected_outputs_path)
        .lines()
        .map(|line| line.expect("error reading expected output"))
        .map(|line| parse_expected_output(&line, ignore))
        .collect()
}
