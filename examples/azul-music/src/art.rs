//! The art of an album, an artist, a genre or a playlist that has no picture: its initials on a
//! colour of its own - the same name, the same colour, every run. Plain Rust, tested without a
//! window.

/// The colours a name can get: Spotify's lime first, then the deep tones of its old covers.
pub const COLOURS: [(u8, u8, u8); 12] = [
    (0x84, 0xbd, 0x00),
    (0xe1, 0x33, 0x00),
    (0x1e, 0x32, 0x64),
    (0x84, 0x00, 0xe7),
    (0xe8, 0x11, 0x5b),
    (0x14, 0x8a, 0x08),
    (0xbc, 0x59, 0x00),
    (0x50, 0x9b, 0xf5),
    (0xaf, 0x28, 0x96),
    (0x27, 0x85, 0x6a),
    (0x47, 0x7d, 0x95),
    (0x8d, 0x67, 0xab),
];

/// A stable hash of `seed` (FNV-1a over its trimmed lowercase): no case or stray space changes
/// a colour.
#[must_use]
pub fn hash(seed: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.trim().to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The colour of `seed`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn colour(seed: &str) -> (u8, u8, u8) {
    COLOURS[(hash(seed) % COLOURS.len() as u64) as usize]
}

/// Up to two initials of `label`, upper case ("Blue Hour" is "BH", "The Beatles" is "B"); words
/// that start with a mark (a bracket, a quote) are skipped; "" when there is no word.
#[must_use]
pub fn initials(label: &str) -> String {
    let words: Vec<&str> = label
        .split_whitespace()
        .filter(|w| w.chars().next().is_some_and(char::is_alphanumeric))
        .collect();
    let words = if words.len() > 1 && words[0].eq_ignore_ascii_case("the") {
        &words[1..]
    } else {
        &words[..]
    };
    words
        .iter()
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

/// `colour` darker (`factor` < 1) or lighter.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn shade((r, g, b): (u8, u8, u8), factor: f32) -> (u8, u8, u8) {
    let f = |c: u8| (f32::from(c) * factor).round().clamp(0.0, 255.0) as u8;
    (f(r), f(g), f(b))
}

/// `#rrggbb`.
#[must_use]
pub fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `rgba(r, g, b, alpha)`.
#[must_use]
pub fn rgba((r, g, b): (u8, u8, u8), alpha: f32) -> String {
    format!("rgba({r}, {g}, {b}, {alpha})")
}

/// The art's CSS background: the colour, darker towards the bottom (the gloss of 2010's covers).
#[must_use]
pub fn background(seed: &str) -> String {
    let c = colour(seed);
    format!(
        "linear-gradient(to bottom, {}, {})",
        hex(shade(c, 1.15)),
        hex(shade(c, 0.5))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_are_the_first_letters_of_the_first_two_words() {
        assert_eq!(initials("Blue Hour"), "BH");
        assert_eq!(initials("ada park"), "AP");
        assert_eq!(initials("Northlight Quartet Live"), "NQ");
        assert_eq!(initials("The Beatles"), "B");
        assert_eq!(initials("The"), "T", "a lone 'The' is a name");
        assert_eq!(initials("(What's the Story) Morning Glory"), "SM");
        assert_eq!(initials("   "), "");
        assert_eq!(initials("\u{e9}t\u{e9}"), "\u{c9}");
    }

    #[test]
    fn a_name_has_one_colour_whatever_its_case_or_spaces() {
        assert_eq!(colour("Blue Hour"), colour(" blue hour "));
        assert!(COLOURS.contains(&colour("anything")));
        let used: std::collections::BTreeSet<(u8, u8, u8)> =
            ["Blue Hour", "Field Notes", "Jazz", "Ambient", "Mix", "Ada Park", "Focus", "Rock"]
                .iter()
                .map(|n| colour(n))
                .collect();
        assert!(used.len() > 2, "names spread over the colours: {used:?}");
    }

    #[test]
    fn colours_read_as_css() {
        assert_eq!(hex((0x84, 0xbd, 0x00)), "#84bd00");
        assert_eq!(shade((100, 200, 250), 0.5), (50, 100, 125));
        assert_eq!(shade((200, 200, 200), 2.0), (255, 255, 255), "lighter stops at white");
        assert_eq!(rgba((1, 2, 3), 0.5), "rgba(1, 2, 3, 0.5)");
        assert!(background("Blue Hour").starts_with("linear-gradient(to bottom, #"));
    }
}
