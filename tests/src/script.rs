#[cfg(test)]
use azul_layout::text2::script::*;

#[test]
fn test_detect_script() {
    assert_eq!(detect_script("1234567890-,;!"), None);

    // One script
    assert_eq!(detect_script("Hello!"), Some(Script::Latin));
    assert_eq!(detect_script("Привет всем!"), Some(Script::Cyrillic));
    assert_eq!(
        detect_script("ქართული ენა მსოფლიო "),
        Some(Script::Georgian)
    );
    assert_eq!(
        detect_script("県見夜上温国阪題富販"),
        Some(Script::Mandarin)
    );
    assert_eq!(
        detect_script(" ككل حوالي 1.6، ومعظم الناس "),
        Some(Script::Arabic)
    );
    assert_eq!(
        detect_script("हिमालयी वन चिड़िया (जूथेरा सालिमअली) चिड़िया की एक प्रजाति है"),
        Some(Script::Devanagari)
    );
    assert_eq!(
        detect_script("היסטוריה והתפתחות של האלפבית העברי"),
        Some(Script::Hebrew)
    );
    assert_eq!(
        detect_script("የኢትዮጵያ ፌዴራላዊ ዴሞክራሲያዊሪፐብሊክ"),
        Some(Script::Ethiopic)
    );

    // Mixed scripts
    assert_eq!(
        detect_script("Привет! Текст на русском with some English."),
        Some(Script::Cyrillic)
    );
    assert_eq!(
        detect_script("Russian word любовь means love."),
        Some(Script::Latin)
    );
}

#[test]
fn test_is_latin() {
    assert_eq!(is_latin('z'), true);
    assert_eq!(is_latin('A'), true);
    assert_eq!(is_latin('č'), true);
    assert_eq!(is_latin('š'), true);
    assert_eq!(is_latin('Ĵ'), true);

    assert_eq!(is_latin('ж'), false);
}

#[test]
fn test_is_cyrillic() {
    assert_eq!(is_cyrillic('а'), true);
    assert_eq!(is_cyrillic('Я'), true);
    assert_eq!(is_cyrillic('Ґ'), true);
    assert_eq!(is_cyrillic('ї'), true);
    assert_eq!(is_cyrillic('Ꙕ'), true);

    assert_eq!(is_cyrillic('L'), false);
}

#[test]
fn test_is_ethiopic() {
    assert_eq!(is_ethiopic('ፚ'), true);
    assert_eq!(is_ethiopic('ᎀ'), true);

    assert_eq!(is_ethiopic('а'), false);
    assert_eq!(is_ethiopic('L'), false);
}

#[test]
fn test_is_georgian() {
    assert_eq!(is_georgian('რ'), true);
    assert_eq!(is_georgian('ж'), false);
}

#[test]
fn test_is_bengali() {
    assert_eq!(is_bengali('ই'), true);
    assert_eq!(is_bengali('z'), false);
}

#[test]
fn test_is_katakana() {
    assert_eq!(is_katakana('カ'), true);
    assert_eq!(is_katakana('f'), false);
}

#[test]
fn test_is_hiragana() {
    assert_eq!(is_hiragana('ひ'), true);
    assert_eq!(is_hiragana('a'), false);
}

#[test]
fn test_is_hangul() {
    assert_eq!(is_hangul('ᄁ'), true);
    assert_eq!(is_hangul('t'), false);
}

#[test]
fn test_is_greek() {
    assert_eq!(is_greek('φ'), true);
    assert_eq!(is_greek('ф'), false);
}

#[test]
fn test_is_kannada() {
    assert_eq!(is_kannada('ಡ'), true);
    assert_eq!(is_kannada('S'), false);
}

#[test]
fn test_is_tamil() {
    assert_eq!(is_tamil('ஐ'), true);
    assert_eq!(is_tamil('Ж'), false);
}

#[test]
fn test_is_thai() {
    assert_eq!(is_thai('ก'), true);
    assert_eq!(is_thai('๛'), true);
    assert_eq!(is_thai('Ж'), false);
}

#[test]
fn test_is_gujarati() {
    assert_eq!(is_gujarati('ઁ'), true);
    assert_eq!(is_gujarati('૱'), true);
    assert_eq!(is_gujarati('Ж'), false);
}

#[test]
fn test_is_gurmukhi() {
    assert_eq!(is_gurmukhi('ਁ'), true);
    assert_eq!(is_gurmukhi('ੴ'), true);
    assert_eq!(is_gurmukhi('Ж'), false);
}

#[test]
fn test_is_telugu() {
    assert_eq!(is_telugu('ఁ'), true);
    assert_eq!(is_telugu('౿'), true);
    assert_eq!(is_telugu('Ж'), false);
}

#[test]
fn test_is_oriya() {
    assert_eq!(is_oriya('ଐ'), true);
    assert_eq!(is_oriya('୷'), true);
    assert_eq!(is_oriya('౿'), false);
}
