//! What a window's desktop is: a wallpaper is known by its picture and its
//! screen (not by a count of the wallpapers made), a desktop takes what its
//! reader finds and moves its generation only when that differs, and no
//! desktop sees another's.

use super::*;

/// The screen every wallpaper of these tests covers.
const SCREEN: [f32; 4] = [0.0, 0.0, 100.0, 100.0];

/// A wallpaper of one grey over `screen`.
fn grey(value: u8, screen: [f32; 4]) -> Wallpaper {
    Wallpaper::from_rgba8(2, 2, &[value; 16], screen).expect("a wallpaper")
}

/// A reader that finds one grey wallpaper.
fn reads_a_dark_grey() -> Vec<Wallpaper> {
    vec![grey(20, SCREEN)]
}

/// A reader that finds another one.
fn reads_a_light_grey() -> Vec<Wallpaper> {
    vec![grey(200, SCREEN)]
}

#[test]
fn a_wallpaper_is_known_by_its_picture_and_its_screen() {
    assert_eq!(
        grey(20, SCREEN).id,
        grey(20, SCREEN).id,
        "the same picture over the same screen is the same wallpaper, wherever it was read"
    );
    assert_ne!(grey(20, SCREEN).id, grey(21, SCREEN).id, "another picture is another wallpaper");
    assert_ne!(
        grey(20, SCREEN).id,
        grey(20, [100.0, 0.0, 100.0, 100.0]).id,
        "so is the same picture over another screen, which a frame reflects differently"
    );
}

#[test]
fn a_desktop_reads_once_and_moves_its_generation_only_when_what_it_reads_differs() {
    let desktop = DesktopWallpapers::new();
    assert_eq!(desktop.generation(), 0);
    assert!(
        matches!(desktop.environment([1, 2, 3]), Environment::Highlight(_)),
        "no reader and no wallpaper: a metal reflects the highlight"
    );

    desktop.set_loader(reads_a_dark_grey);
    assert_eq!(desktop.generation(), 0, "nothing is read until a metal asks");
    let Environment::Wallpapers(list) = desktop.environment([1, 2, 3]) else {
        panic!("a metal reflects the wallpaper the reader found");
    };
    assert_eq!(list.len(), 1);
    assert_eq!(desktop.generation(), 1, "the first reading is a change");

    assert!(!desktop.refresh(), "the same wallpaper read again is no change");
    assert_eq!(desktop.generation(), 1);

    desktop.set_loader(reads_a_light_grey);
    assert!(desktop.refresh(), "another picture is");
    assert_eq!(desktop.generation(), 2);
}

#[test]
fn a_desktop_has_the_wallpapers_it_was_given_and_no_other_desktop_does() {
    let (mine, yours) = (DesktopWallpapers::new(), DesktopWallpapers::new());
    mine.set_wallpapers(reads_a_dark_grey());
    assert_eq!(mine.generation(), 1);
    assert!(matches!(mine.environment([1, 2, 3]), Environment::Wallpapers(_)));

    assert_eq!(yours.generation(), 0, "the wallpapers of another desktop are not this one's");
    assert!(matches!(yours.environment([1, 2, 3]), Environment::Highlight(_)));
    assert!(!yours.refresh(), "a desktop without a reader has nothing to read again");
}
