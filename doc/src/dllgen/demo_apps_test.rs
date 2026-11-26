use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use super::*;
use crate::dllgen::deploy;

const VERSION: &str = "0.2.0";

fn manifest() -> Manifest {
    Manifest::embedded()
}

fn app_of<'a>(m: &'a Manifest, package: &str) -> &'a App {
    m.app(package)
        .unwrap_or_else(|| panic!("{package} is not in scripts/demo_apps.toml"))
}

/// An app made by hand, for the cases the real manifest does not hold.
fn made(package: &str, platforms: &[Platform]) -> App {
    App {
        package: package.to_string(),
        dir: format!("examples/{}", package.to_lowercase()),
        name: None,
        description: format!("the {package} app"),
        platforms: platforms.to_vec(),
        weight: 1,
        docker: false,
        needs: Vec::new(),
        known_failing: BTreeMap::new(),
        mobile_lib: None,
    }
}

fn with_known_failing(mut app: App, key: &str, why: &str) -> App {
    app.known_failing.insert(key.to_string(), why.to_string());
    app
}

fn manifest_of(apps: Vec<App>) -> Manifest {
    Manifest {
        schema: SCHEMA,
        apps,
    }
}

/// The section of the page, with the routing of the real release page.
fn section(m: &Manifest) -> String {
    demos_section_html(VERSION, m, deploy::asset_url)
}

/// The html of the row of `package`, from its `<tr>` to the end of the row.
fn row<'a>(html: &'a str, package: &str) -> &'a str {
    let open = format!("<tr id='demo-{package}'>");
    let start = html
        .find(&open)
        .unwrap_or_else(|| panic!("the section has no row {open}"));
    let end = html[start..]
        .find("</tr>")
        .map(|i| start + i)
        .expect("a row is closed");
    &html[start..end]
}

/// The html of the cell of `column` in a row.
fn cell_of<'a>(row: &'a str, column: Column) -> &'a str {
    let marker = format!("data-column='{}'", column.id());
    let at = row
        .find(&marker)
        .unwrap_or_else(|| panic!("the row has no cell {marker}: {row}"));
    let start = row[..at]
        .rfind("<td")
        .expect("a cell opens before its column");
    let end = row[at..]
        .find("</td>")
        .map(|i| at + i + "</td>".len())
        .expect("a cell is closed");
    &row[start..end]
}

/// `python3 scripts/demo_apps.py assets <package>`, as that script prints it: app, column, staged
/// name and release name, tab-separated, a line per built file.
fn assets_lines(m: &Manifest, package: &str) -> String {
    let app = app_of(m, package);
    columns_of(app)
        .into_iter()
        .map(|c| {
            format!(
                "{}\t{}\t{}\t{}\n",
                app.package,
                c.id(),
                staged_name(app, c),
                release_name(app, c)
            )
        })
        .collect()
}

/// What `python3 scripts/demo_apps.py assets AzWidgets` prints. scripts/test_demo_apps.py holds the
/// same lines against the script itself, and a test below holds this file to that one: the names of
/// the built files are spelled in three places (the script, this module, the page) and these lines are
/// where the three are pinned to each other.
const GOLDEN_AZWIDGETS: &str = "\
AzWidgets\tlinux\tAzWidgets-linux\tAzWidgets-linux.tar.gz
AzWidgets\tmacos-arm64\tAzWidgets-macos\tAzWidgets-macos.tar.gz
AzWidgets\tmacos-intel\tAzWidgets-macos-intel\tAzWidgets-macos-intel.tar.gz
AzWidgets\twindows\tAzWidgets-windows.exe\tAzWidgets-windows.exe.tar.gz
AzWidgets\tandroid-arm64\tAzWidgets-android-arm64.apk\tAzWidgets-android-arm64.apk
AzWidgets\tandroid-x86_64\tAzWidgets-android-x86_64.apk\tAzWidgets-android-x86_64.apk
AzWidgets\tios-device\tAzWidgets-ios-device.app.zip\tAzWidgets-ios-device.app.zip
AzWidgets\tios-sim\tAzWidgets-ios-sim.app.zip\tAzWidgets-ios-sim.app.zip
";

/// What `python3 scripts/demo_apps.py assets AzCompanion` prints (a phone-only app).
const GOLDEN_AZCOMPANION: &str = "\
AzCompanion\tandroid-arm64\tAzCompanion-android-arm64.apk\tAzCompanion-android-arm64.apk
AzCompanion\tandroid-x86_64\tAzCompanion-android-x86_64.apk\tAzCompanion-android-x86_64.apk
AzCompanion\tios-device\tAzCompanion-ios-device.app.zip\tAzCompanion-ios-device.app.zip
AzCompanion\tios-sim\tAzCompanion-ios-sim.app.zip\tAzCompanion-ios-sim.app.zip
";

// ---------------------------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------------------------

#[test]
fn the_manifest_that_is_compiled_in_has_no_problems() {
    let m = manifest();
    assert_eq!(m.problems(), Vec::<String>::new());
    assert!(m.apps.len() > 30, "{} apps", m.apps.len());
}

#[test]
fn every_platform_and_column_is_spelled_the_way_the_manifest_and_the_status_file_spell_it() {
    for p in PLATFORMS.iter().copied() {
        assert_eq!(Platform::from_id(p.id()), Some(p));
    }
    for c in COLUMNS.iter().copied() {
        assert_eq!(Column::from_id(c.id()), Some(c));
        // a desktop column is its own platform; a phone column is one of the two files of its platform
        assert_eq!(c.is_desktop(), c.id() == c.platform().id());
    }
    let ids: Vec<&str> = COLUMNS.iter().map(|c| c.id()).collect();
    assert_eq!(
        ids,
        [
            "linux",
            "macos-arm64",
            "macos-intel",
            "windows",
            "android-arm64",
            "android-x86_64",
            "ios-device",
            "ios-sim"
        ]
    );
    let mut sorted = COLUMNS.to_vec();
    sorted.sort();
    assert_eq!(
        sorted,
        COLUMNS.to_vec(),
        "the columns are in the order of their declaration"
    );
    assert_eq!(
        Column::from_id("android"),
        None,
        "a platform is not a column"
    );
    assert_eq!(
        Platform::from_id("ios-sim"),
        None,
        "a column is not a platform"
    );
}

#[test]
fn a_problem_is_named_for_every_way_the_page_could_not_render_an_app() {
    let mut app = made("Azbad", &[]);
    app.description = String::new();
    app.known_failing
        .insert("android".to_string(), "  ".to_string());
    app.known_failing
        .insert("amiga".to_string(), "no".to_string());
    let m = Manifest {
        schema: 2,
        apps: vec![app.clone(), app],
    };
    let problems = m.problems().join("\n");
    for needle in [
        "schema is 2",
        "Azbad is listed twice",
        "breaks the AzXxx convention",
        "the description is empty",
        "builds for no platform",
        "known_failing.android needs the reason",
        "\"amiga\", which is no column or platform",
    ] {
        assert!(
            problems.contains(needle),
            "no problem names {needle:?}:\n{problems}"
        );
    }
    assert_eq!(
        Manifest {
            schema: SCHEMA,
            apps: vec![]
        }
        .problems(),
        ["the manifest names no app"]
    );
    assert!(is_az_name("AzERP") && is_az_name("AzPdf") && is_az_name("AzX9"));
    for bad in [
        "",
        "Az",
        "AzA b",
        "Azul",
        "azMaps",
        "AzMaps-linux",
        "Ax",
        "Az_Maps",
        "AzÄ",
    ] {
        assert!(!is_az_name(bad), "{bad:?}");
    }
}

#[test]
fn an_app_is_shown_by_its_name_and_by_its_package_when_it_has_none() {
    let m = manifest();
    for app in &m.apps {
        assert_eq!(
            app.display_name(),
            app.name.as_deref().unwrap_or(&app.package)
        );
    }
    let mut app = made("AzBare", &[Platform::Linux]);
    assert_eq!(app.display_name(), "AzBare");
    app.name = Some("The Bare App".to_string());
    assert_eq!(app.display_name(), "The Bare App");
    // the files keep the package's name, whatever the page calls the app
    assert_eq!(staged_name(&app, Column::Linux), "AzBare-linux");
    let html = section(&manifest_of(vec![app]));
    assert!(html.contains("<strong>The Bare App</strong>"), "{html}");
    assert!(html.contains("<tr id='demo-AzBare'>"), "{html}");
    assert!(html.contains("AzBare-linux.tar.gz"), "{html}");
    assert!(
        html.contains("aria-label='The Bare App for Linux'"),
        "{html}"
    );
}

// ---------------------------------------------------------------------------------------------
// The names of the built files
// ---------------------------------------------------------------------------------------------

#[test]
fn desktop_files_are_named_after_the_package_and_the_platform() {
    let desktop = [
        Column::Linux,
        Column::MacosArm64,
        Column::MacosIntel,
        Column::Windows,
    ];
    let a = made(
        "AzDesk",
        &[
            Platform::Linux,
            Platform::MacosArm64,
            Platform::MacosIntel,
            Platform::Windows,
        ],
    );
    assert_eq!(staged_name(&a, Column::Linux), "AzDesk-linux");
    assert_eq!(staged_name(&a, Column::MacosArm64), "AzDesk-macos");
    assert_eq!(staged_name(&a, Column::MacosIntel), "AzDesk-macos-intel");
    assert_eq!(staged_name(&a, Column::Windows), "AzDesk-windows.exe");
    assert_eq!(release_name(&a, Column::Linux), "AzDesk-linux.tar.gz");
    assert_eq!(
        release_name(&a, Column::Windows),
        "AzDesk-windows.exe.tar.gz"
    );
    for c in desktop {
        assert!(c.is_desktop(), "{}", c.id());
        assert_eq!(
            release_name(&a, c),
            format!("{}.tar.gz", staged_name(&a, c))
        );
        assert_eq!(asset_path(&a, c), format!("demos/{}", staged_name(&a, c)));
    }
}

#[test]
fn the_phone_files_have_the_names_azreview_downloads_and_are_not_tarred() {
    let a = made("AzPhone", &[Platform::Android, Platform::Ios]);
    assert_eq!(
        staged_name(&a, Column::AndroidArm64),
        "AzPhone-android-arm64.apk"
    );
    assert_eq!(
        staged_name(&a, Column::AndroidX86_64),
        "AzPhone-android-x86_64.apk"
    );
    assert_eq!(
        staged_name(&a, Column::IosDevice),
        "AzPhone-ios-device.app.zip"
    );
    assert_eq!(staged_name(&a, Column::IosSim), "AzPhone-ios-sim.app.zip");
    for c in [
        Column::AndroidArm64,
        Column::AndroidX86_64,
        Column::IosDevice,
        Column::IosSim,
    ] {
        assert_eq!(release_name(&a, c), staged_name(&a, c), "{}", c.id());
        assert_eq!(
            asset_path(&a, c),
            format!("mobile-apps/{}", staged_name(&a, c))
        );
        assert!(!c.is_desktop());
    }
}

#[test]
fn the_old_phone_names_are_gone_from_the_page() {
    let m = manifest();
    let html = section(&m);
    for old in [
        "-ios.app.zip",
        "-android.apk",
        "-ios.ipa",
        ".ipa",
        "mobile-apps/",
    ] {
        assert!(!html.contains(old), "the page still has {old:?}");
    }
    for app in &m.apps {
        for c in columns_of(app) {
            for name in [staged_name(app, c), release_name(app, c)] {
                for old in ["-ios.app.zip", "-android.apk", "-ios.ipa"] {
                    assert!(
                        !name.ends_with(old),
                        "{name} is spelled the way the page used to spell it"
                    );
                }
            }
        }
    }
}

#[test]
fn a_phone_app_builds_both_files_of_each_phone_platform() {
    let phone = made("AzPhone", &[Platform::Android, Platform::Ios]);
    assert_eq!(
        columns_of(&phone),
        [
            Column::AndroidArm64,
            Column::AndroidX86_64,
            Column::IosDevice,
            Column::IosSim
        ]
    );
    let droid = made("AzDroid", &[Platform::Android]);
    assert_eq!(
        columns_of(&droid),
        [Column::AndroidArm64, Column::AndroidX86_64]
    );
    let desktop = made(
        "AzDesk",
        &[
            Platform::Linux,
            Platform::MacosArm64,
            Platform::MacosIntel,
            Platform::Windows,
        ],
    );
    assert_eq!(
        columns_of(&desktop),
        [
            Column::Linux,
            Column::MacosArm64,
            Column::MacosIntel,
            Column::Windows
        ]
    );
    assert_eq!(columns_of(&made("AzAll", &PLATFORMS)), COLUMNS.to_vec());
    // and in the real manifest the files of an app are what its platforms say: one for a desktop
    // platform, two for a phone platform
    for app in &manifest().apps {
        let platforms: BTreeSet<Platform> = app.platforms.iter().copied().collect();
        let want: usize = platforms
            .iter()
            .map(|p| match p {
                Platform::Android | Platform::Ios => 2,
                _ => 1,
            })
            .sum();
        assert_eq!(columns_of(app).len(), want, "{}", app.package);
    }
}

#[test]
fn the_columns_of_an_app_come_in_column_order_whatever_the_order_of_its_platforms() {
    let app = made(
        "AzMixed",
        &[
            Platform::Ios,
            Platform::Windows,
            Platform::Android,
            Platform::Linux,
            Platform::Linux,
        ],
    );
    assert_eq!(
        columns_of(&app),
        [
            Column::Linux,
            Column::Windows,
            Column::AndroidArm64,
            Column::AndroidX86_64,
            Column::IosDevice,
            Column::IosSim
        ]
    );
}

#[test]
fn no_two_assets_share_a_name() {
    let m = manifest();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut lower: BTreeMap<String, String> = BTreeMap::new();
    let mut count = 0;
    for app in &m.apps {
        for c in columns_of(app) {
            let who = format!("{} {}", app.package, c.id());
            for name in BTreeSet::from([staged_name(app, c), release_name(app, c)]) {
                if let Some(other) = seen.insert(name.clone(), who.clone()) {
                    panic!("{name}: {who} and {other}");
                }
                // a Mac or a Windows machine keeps files that differ in case in one place
                if let Some(other) = lower.insert(name.to_lowercase(), who.clone()) {
                    panic!("{name} differs only in case from a name of {other}");
                }
                count += 1;
            }
        }
    }
    println!("{count} asset names, all different");
    assert!(count > 100);
}

#[test]
fn the_names_are_the_ones_scripts_demo_apps_py_prints() {
    let m = manifest();
    assert_eq!(assets_lines(&m, "AzWidgets"), GOLDEN_AZWIDGETS);
    assert_eq!(assets_lines(&m, "AzCompanion"), GOLDEN_AZCOMPANION);
}

#[test]
fn the_two_path_prefixes_are_names_that_route_to_release_assets() {
    // nothing is stored under demos/ or mobile-apps/: they are the names the page links from, and
    // they stay LARGE (a release asset), a desktop demo tarred and a phone file not
    assert!(deploy::is_large("demos/AzX-linux") && deploy::is_tarred("demos/AzX-linux"));
    assert!(
        deploy::is_large("demos/AzX-windows.exe") && deploy::is_tarred("demos/AzX-windows.exe")
    );
    assert!(deploy::is_large("mobile-apps/AzX-android-arm64.apk"));
    assert!(!deploy::is_tarred("mobile-apps/AzX-android-arm64.apk"));
    assert!(deploy::is_large("mobile-apps/AzX-ios-sim.app.zip"));
    assert!(!deploy::is_tarred("mobile-apps/AzX-ios-sim.app.zip"));
    assert_eq!(
        deploy::asset_url("1.2.3", "demos/AzX-linux"),
        "https://github.com/fschutt/azul/releases/download/1.2.3/AzX-linux.tar.gz"
    );
    assert_eq!(
        deploy::asset_url("1.2.3", "mobile-apps/AzX-ios-device.app.zip"),
        "https://github.com/fschutt/azul/releases/download/1.2.3/AzX-ios-device.app.zip"
    );
}

#[test]
fn the_asset_routing_gives_the_release_name_for_every_app_and_column() {
    let m = manifest();
    let mut checked = 0;
    for app in &m.apps {
        for column in columns_of(app) {
            let path = asset_path(app, column);
            assert!(
                path.starts_with(if column.is_desktop() {
                    "demos/"
                } else {
                    "mobile-apps/"
                }),
                "{path}"
            );
            let url = deploy::asset_url(VERSION, &path);
            let want = format!(
                "https://github.com/fschutt/azul/releases/download/{VERSION}/{}",
                release_name(app, column)
            );
            assert_eq!(url, want, "{} {}", app.package, column.id());
            checked += 1;
        }
    }
    println!("{checked} files route to the GitHub release under their release name");
    assert_eq!(checked, numbers(&m).built_files);
}

#[test]
fn a_blank_reason_names_no_known_failure_and_a_column_is_asked_before_its_platform() {
    let app = with_known_failing(
        made("AzX", &[Platform::Android, Platform::Ios, Platform::Linux]),
        "android",
        "no cdylib",
    );
    assert_eq!(known_failing(&app, Column::AndroidArm64), Some("no cdylib"));
    assert_eq!(
        known_failing(&app, Column::AndroidX86_64),
        Some("no cdylib")
    );
    assert_eq!(known_failing(&app, Column::IosSim), None);
    assert_eq!(known_failing(&app, Column::Linux), None);

    let app = with_known_failing(app, "android-arm64", "the arm64 library is missing");
    assert_eq!(
        known_failing(&app, Column::AndroidArm64),
        Some("the arm64 library is missing")
    );
    assert_eq!(
        known_failing(&app, Column::AndroidX86_64),
        Some("no cdylib")
    );

    let app = with_known_failing(made("AzY", &[Platform::Ios]), "ios", " ");
    assert_eq!(known_failing(&app, Column::IosDevice), None);
    let app = with_known_failing(
        with_known_failing(made("AzZ", &[Platform::Ios]), "ios-sim", ""),
        "ios",
        "signing",
    );
    assert_eq!(
        known_failing(&app, Column::IosSim),
        Some("signing"),
        "a blank column reason falls through to the platform's"
    );
}

// ---------------------------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------------------------

#[test]
fn the_section_lists_every_app_once_in_the_order_of_the_manifest() {
    let m = manifest();
    let html = section(&m);
    let table = downloads_table_html(VERSION, &m, deploy::asset_url);
    let rows: Vec<&str> = table
        .split("<tr id='demo-")
        .skip(1)
        .map(|r| r.split('\'').next().unwrap())
        .collect();
    let want: Vec<&str> = m.apps.iter().map(|a| a.package.as_str()).collect();
    assert_eq!(rows, want);
    assert_eq!(html.matches("<tr id='demo-").count(), m.apps.len());
    for app in &m.apps {
        assert_eq!(
            html.matches(&format!("<tr id='demo-{}'>", app.package))
                .count(),
            1,
            "{} has one row",
            app.package
        );
        let r = row(&html, &app.package);
        assert!(
            r.contains(&format!("<strong>{}</strong>", app.display_name())),
            "{r}"
        );
        assert_eq!(
            r.matches("<td ").count(),
            COLUMNS.len(),
            "{}: one cell per column",
            app.package
        );
    }
}

#[test]
fn the_cells_of_a_row_are_in_the_order_of_the_headings() {
    let m = manifest();
    let table = downloads_table_html(VERSION, &m, deploy::asset_url);
    let head = &table[..table.find("</thead>").expect("a head")];
    let ids = |text: &str| -> Vec<String> {
        text.split("data-column='")
            .skip(1)
            .map(|p| p.split('\'').next().unwrap().to_string())
            .collect()
    };
    let want: Vec<String> = COLUMNS.iter().map(|c| c.id().to_string()).collect();
    assert_eq!(ids(head), want);
    for app in &m.apps {
        assert_eq!(ids(row(&table, &app.package)), want, "{}", app.package);
    }
    assert!(
        head.contains("scope='col'") && row(&table, &m.apps[0].package).contains("<th scope='row'")
    );
}

#[test]
fn every_heading_says_what_the_file_is_and_the_ios_device_one_says_who_signs_it() {
    let table = downloads_table_html(VERSION, &manifest(), deploy::asset_url);
    for c in COLUMNS.iter().copied() {
        assert!(
            table.contains(&format!(
                "title='{}'>{}<small>{}</small></th>",
                c.hint(),
                c.heading(),
                c.note()
            )),
            "{}",
            c.id()
        );
        assert!(!c.hint().is_empty() && !c.note().is_empty());
    }
    assert_eq!(Column::IosDevice.note(), "unsigned .app");
    assert_eq!(
        Column::IosDevice.hint(),
        "an unsigned .app (zipped): AzReview signs it for your phone"
    );
    assert!(
        intro_html().contains("The iOS device .app is unsigned: AzReview signs it for your phone.")
    );
    assert_eq!(Column::AndroidArm64.heading(), "Android arm64 APK");
    assert_eq!(Column::AndroidX86_64.heading(), "Android emulator APK");
    assert_eq!(Column::MacosArm64.heading(), "macOS Apple Silicon");
    assert_eq!(Column::MacosIntel.heading(), "macOS Intel");
    assert_eq!(Column::IosDevice.heading(), "iOS device");
    assert_eq!(Column::IosSim.heading(), "iOS Simulator .app");
}

#[test]
fn the_table_sits_in_a_box_that_scrolls_on_a_narrow_screen() {
    let table = downloads_table_html(VERSION, &manifest(), deploy::asset_url);
    assert!(
        table.starts_with("<div class='release-demo-table'"),
        "{}",
        &table[..80]
    );
    assert!(
        table.contains("tabindex='0'"),
        "a scrolling box has to be reachable with the keyboard"
    );
    assert!(table.ends_with("</table>\n</div>"));
}

#[test]
fn a_link_exists_for_exactly_the_built_columns_that_are_not_known_failing() {
    let m = manifest();
    let html = section(&m);
    let mut links = 0;
    let mut dashes = 0;
    let mut unavailable = 0;
    for app in &m.apps {
        let r = row(&html, &app.package);
        let built = columns_of(app);
        for column in COLUMNS.iter().copied() {
            let c = cell_of(r, column);
            let failing = known_failing(app, column).is_some();
            let want_link = built.contains(&column) && !failing;
            assert_eq!(
                c.matches("<a ").count(),
                usize::from(want_link),
                "{} {}: {c}",
                app.package,
                column.id()
            );
            assert_eq!(
                c.contains("href"),
                want_link,
                "{} {}: {c}",
                app.package,
                column.id()
            );
            if want_link {
                let url = deploy::asset_url(VERSION, &asset_path(app, column));
                assert!(
                    c.contains(&format!("<a href='{url}'")),
                    "{} {}: wanted {url} in {c}",
                    app.package,
                    column.id()
                );
                assert!(
                    c.contains(&format!("title='{}'", release_name(app, column))),
                    "{c}"
                );
                assert!(c.contains(&format!(">{}</a>", column.file_kind())), "{c}");
                links += 1;
            } else if !built.contains(&column) {
                assert!(
                    c.contains("class='demo-none'") && c.contains(">-</td>"),
                    "{} {}: {c}",
                    app.package,
                    column.id()
                );
                dashes += 1;
            } else {
                unavailable += 1;
            }
        }
    }
    let n = numbers(&m);
    assert_eq!(
        (links, dashes, unavailable),
        (n.downloads, n.dashes, n.unavailable),
        "{n}"
    );
}

/// What a cell of a build that the manifest lists as failing holds: the reason, "not available", and
/// no link.
fn assert_known_failing_cell(c: &str, reason: &str, what: &str) {
    assert!(
        c.contains(&esc(reason)),
        "{what}: the reason {reason:?} is not in {c}"
    );
    assert!(c.contains("not available"), "{what}: {c}");
    assert!(c.contains("is-missing"), "{what}: {c}");
    assert!(
        !c.contains("href") && !c.contains("<a "),
        "{what}: a build that fails has no link: {c}"
    );
    assert!(
        c.contains(&format!("title='not available: {}'", esc(reason))),
        "{what}: {c}"
    );
}

#[test]
fn a_known_failing_cell_has_the_reason_and_no_link() {
    // the real manifest: every build it lists as failing (none at all once every app builds)
    let m = manifest();
    let real = section(&m);
    let mut listed = 0;
    for app in &m.apps {
        let r = row(&real, &app.package);
        for column in columns_of(app) {
            if let Some(reason) = known_failing(app, column) {
                let what = format!("{} {}", app.package, column.id());
                assert_known_failing_cell(cell_of(r, column), reason, &what);
                listed += 1;
            }
        }
    }
    println!("{listed} builds of the manifest are listed as failing");
    assert_eq!(
        real.matches("<span class='demo-na'>not available</span>")
            .count(),
        listed,
        "every cell that says \"not available\" is a build the manifest lists as failing"
    );

    // a phone-only app whose phone builds are all failing (the key of a platform names both files)
    let phone = with_known_failing(
        with_known_failing(
            made("AzPhone", &[Platform::Android, Platform::Ios]),
            "android",
            "no cdylib yet",
        ),
        "ios",
        "no bin yet",
    );
    let m = manifest_of(vec![phone.clone()]);
    let html = section(&m);
    let r = row(&html, "AzPhone");
    let mut checked = 0;
    for column in columns_of(&phone) {
        let reason = known_failing(&phone, column).expect("the build is listed as failing");
        assert_known_failing_cell(cell_of(r, column), reason, column.id());
        checked += 1;
    }
    assert_eq!(checked, 4);
    // the desktop columns of a phone-only app are dashes, not failures
    for column in [
        Column::Linux,
        Column::MacosArm64,
        Column::MacosIntel,
        Column::Windows,
    ] {
        assert!(
            cell_of(r, column).contains("class='demo-none'"),
            "{}",
            column.id()
        );
    }
    assert_eq!(numbers(&m).unavailable, 4);
    assert_eq!(numbers(&m).downloads, 0);
    assert_eq!(html.matches("<a ").count(), 0, "{html}");
}

#[test]
fn a_reason_given_for_one_column_leaves_the_other_file_of_the_platform_a_link() {
    let m = manifest_of(vec![with_known_failing(
        made("AzTwo", &[Platform::Android, Platform::Linux]),
        "android-x86_64",
        "the emulator build needs <a newer> NDK & more",
    )]);
    let html = section(&m);
    let r = row(&html, "AzTwo");
    assert!(cell_of(r, Column::AndroidArm64).contains(
        "<a href='https://github.com/fschutt/azul/releases/download/0.2.0/AzTwo-android-arm64.apk'"
    ));
    let c = cell_of(r, Column::AndroidX86_64);
    assert!(!c.contains("href"), "{c}");
    // the manifest's text is escaped, the angle brackets and the ampersand do not become markup
    assert!(
        c.contains("the emulator build needs &lt;a newer&gt; NDK &amp; more"),
        "{c}"
    );
    assert!(!c.contains("<a newer>"), "{c}");
}

#[test]
fn the_text_of_the_manifest_is_escaped_and_a_code_span_is_code() {
    let mut app = made("AzText", &[Platform::Linux]);
    app.description = "an <b>app</b> that's \"quoted\" & done".to_string();
    app.needs = vec![
        "a meeting server (the `meet` worker)".to_string(),
        "an `unclosed span".to_string(),
    ];
    app.name = Some("Az <Text>".to_string());
    let html = section(&manifest_of(vec![app]));
    assert!(
        html.contains("an &lt;b&gt;app&lt;/b&gt; that&#39;s &quot;quoted&quot; &amp; done"),
        "{html}"
    );
    assert!(html.contains("<strong>Az &lt;Text&gt;</strong>"), "{html}");
    assert!(
        html.contains(
            "Needs a meeting server (the <code>meet</code> worker); an `unclosed span</span>"
        ),
        "{html}"
    );
    assert!(!html.contains("<b>app</b>"), "{html}");
    assert_eq!(
        inline("a `b` c `d` e"),
        "a <code>b</code> c <code>d</code> e"
    );
    assert_eq!(inline("no code"), "no code");
    assert_eq!(inline("one ` tick"), "one ` tick");
}

#[test]
fn an_app_that_needs_something_says_so_under_its_name() {
    let m = manifest();
    let html = section(&m);
    for app in &m.apps {
        let r = row(&html, &app.package);
        assert_eq!(
            r.contains("demo-needs"),
            !app.needs.is_empty(),
            "{}",
            app.package
        );
        if !app.needs.is_empty() {
            let needs: Vec<String> = app.needs.iter().map(|n| inline(n)).collect();
            assert!(
                r.contains(&format!("Needs {}", needs.join("; "))),
                "{}",
                app.package
            );
        }
        assert!(
            r.contains(&format!(
                "<span class='demo-desc'>{}</span>",
                inline(&app.description)
            )),
            "{}",
            app.package
        );
    }
    let mut app = made("AzNeedy", &[Platform::Linux]);
    app.needs = vec![
        "a network connection for the map tiles".to_string(),
        "a meeting server (the `meet` worker)".to_string(),
    ];
    let html = section(&manifest_of(vec![app]));
    assert!(
        html.contains(
            "<span class='demo-needs'>Needs a network connection for the map tiles; a meeting server (the <code>meet</code> worker)</span>"
        ),
        "{html}"
    );
}

/// The first fault in the nesting of the tags of generated html (every tag is closed, in order), or
/// None. The section writes no void element and no `>` inside an attribute (the text is escaped).
fn unbalanced_tag(html: &str) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let Some(end) = rest.find('>') else {
            return Some("a tag that never ends".to_string());
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name.trim() => {}
                other => return Some(format!("</{name}> closes {other:?}")),
            }
        } else if !tag.ends_with('/') {
            let name = tag
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or_default();
            stack.push(name.to_string());
        }
    }
    if stack.is_empty() {
        None
    } else {
        Some(format!("never closed: {stack:?}"))
    }
}

#[test]
fn the_section_is_well_formed_html_and_every_id_of_it_is_used_once() {
    let m = manifest();
    let html = section(&m);
    assert_eq!(unbalanced_tag(&html), None);
    let ids: Vec<&str> = html
        .split(" id='")
        .skip(1)
        .map(|p| p.split('\'').next().unwrap())
        .collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "an id is used twice: {ids:?}");
    // a row per app, a web row per Docker app and the heading of the web part
    let n = numbers(&m);
    assert_eq!(ids.len(), n.apps + n.web_rows + 1, "{n}");
    // a manifest with every awkward text is well formed too
    let mut app = made("AzText", &[Platform::Linux, Platform::Android]);
    app.description = "an <b>app</b> & \"more\"".to_string();
    app.needs = vec!["a `<code>` span".to_string()];
    app.known_failing
        .insert("android".to_string(), "<i>no</i> cdylib".to_string());
    app.docker = true;
    assert_eq!(unbalanced_tag(&section(&manifest_of(vec![app]))), None);
    // the checker itself finds what it is for
    assert!(unbalanced_tag("<p><b>x</p>").is_some());
    assert!(unbalanced_tag("<p>x").is_some());
    assert!(unbalanced_tag("x</p>").is_some());
    assert!(unbalanced_tag("<p x='1'>x</p>").is_none());
}

// ---------------------------------------------------------------------------------------------
// The web (Docker) rows
// ---------------------------------------------------------------------------------------------

#[test]
fn the_docker_rows_are_exactly_the_docker_apps() {
    let m = manifest();
    let web = web_html(VERSION, &m, deploy::asset_url);
    let rows: Vec<&str> = web
        .split("<li id='demo-web-")
        .skip(1)
        .map(|r| r.split('\'').next().unwrap())
        .collect();
    let want: Vec<&str> = m
        .apps
        .iter()
        .filter(|a| a.docker)
        .map(|a| a.package.as_str())
        .collect();
    assert_eq!(rows, want);
    assert!(!want.is_empty(), "the page has web rows");
    for app in &m.apps {
        // an app without a Dockerfile has no row and no link (the old page linked one for nearly every app)
        let url = deploy::asset_url(VERSION, &dockerfile_name(app));
        assert_eq!(web.contains(&url), app.docker, "{}: {url}", app.package);
        assert_eq!(
            web.contains(&format!("<li id='demo-web-{}'>", app.package)),
            app.docker,
            "{}",
            app.package
        );
    }
}

#[test]
fn a_docker_row_links_the_dockerfile_on_the_pages_tree_and_names_the_commands_that_run_it() {
    let mut app = made("AzDock", &[Platform::Linux]);
    app.docker = true;
    let m = manifest_of(vec![app, made("AzPlain", &[Platform::Linux])]);
    let web = web_html(VERSION, &m, deploy::asset_url);
    let url = "https://azul.rs/ui/release/0.2.0/AzDock.Dockerfile";
    assert_eq!(
        deploy::asset_url(VERSION, "AzDock.Dockerfile"),
        url,
        "a Dockerfile is a small asset on Pages"
    );
    assert!(
        web.contains(&format!("<a href='{url}'>Dockerfile</a>")),
        "{web}"
    );
    assert!(
        web.contains(&format!(
            "docker build {url} -t azdock\ndocker run -p 8080:8080 azdock</code></pre>"
        )),
        "{web}"
    );
    assert!(web.contains("<h3 id='demos-web'>Web (Docker, experimental)</h3>"));
    assert!(!web.contains("AzPlain"), "{web}");

    // the real manifest: the same shape for each of its Docker apps
    let real = manifest();
    let web = web_html(VERSION, &real, deploy::asset_url);
    for app in real.apps.iter().filter(|a| a.docker) {
        let url = deploy::asset_url(VERSION, &dockerfile_name(app));
        let tag = app.package.to_lowercase();
        assert!(
            url.starts_with("https://azul.rs/ui/release/0.2.0/"),
            "{url}"
        );
        assert!(
            web.contains(&format!("<a href='{url}'>Dockerfile</a>")),
            "{}",
            app.package
        );
        assert!(
            web.contains(&format!(
                "docker build {url} -t {tag}\ndocker run -p 8080:8080 {tag}</code></pre>"
            )),
            "{}",
            app.package
        );
    }
}

#[test]
fn without_a_docker_app_the_page_has_no_web_part() {
    let m = manifest_of(vec![made("AzPlain", &[Platform::Linux])]);
    assert_eq!(web_html(VERSION, &m, deploy::asset_url), "");
    let html = section(&m);
    assert!(!html.contains("Docker") && !html.contains("<h3"), "{html}");
}

#[test]
fn the_dockerfiles_the_deploy_copies_are_the_ones_the_page_links() {
    let m = manifest();
    let copies = dockerfile_copies(&m);
    let want: Vec<(String, String)> = m
        .apps
        .iter()
        .filter(|a| a.docker)
        .map(|a| {
            (
                format!("{}/Dockerfile", a.dir),
                format!("{}.Dockerfile", a.package),
            )
        })
        .collect();
    assert_eq!(copies, want);
    let web = web_html(VERSION, &m, deploy::asset_url);
    for (_, published) in &copies {
        assert!(
            web.contains(&format!("/{published}'")),
            "the page does not link {published}"
        );
    }
    // the Dockerfile is published under the PACKAGE name (not the directory's: the old copy wrote
    // azul-paint.Dockerfile where the page linked AzPaint.Dockerfile)
    let mut docker = made("AzDock", &[Platform::Linux]);
    docker.docker = true;
    docker.dir = "examples/azul-dock/".to_string();
    let plain = made("AzPlain", &[Platform::Linux]);
    assert_eq!(
        dockerfile_copies(&manifest_of(vec![docker, plain])),
        [(
            "examples/azul-dock/Dockerfile".to_string(),
            "AzDock.Dockerfile".to_string()
        )]
    );
}

// ---------------------------------------------------------------------------------------------
// The numbers
// ---------------------------------------------------------------------------------------------

#[test]
fn the_links_of_the_section_are_the_count_the_manifest_implies() {
    let m = manifest();
    let n = numbers(&m);
    let html = section(&m);
    let anchors = html.matches("<a ").count();
    println!("demo section: {n}");
    assert_eq!(
        anchors,
        n.links(),
        "the section has {anchors} links but the manifest implies {}: {n}",
        n.links()
    );
    assert_eq!(
        html.matches("<a href='https://github.com/fschutt/azul/releases/download/")
            .count(),
        n.downloads,
        "{n}"
    );
    assert_eq!(
        html.matches("<a href='https://azul.rs/ui/release/").count(),
        n.web_rows,
        "{n}"
    );
    // the same numbers counted without numbers()
    let built: usize = m.apps.iter().map(|a| columns_of(a).len()).sum();
    let failing: usize = m
        .apps
        .iter()
        .map(|a| {
            columns_of(a)
                .into_iter()
                .filter(|c| known_failing(a, *c).is_some())
                .count()
        })
        .sum();
    assert_eq!(n.apps, m.apps.len());
    assert_eq!(n.built_files, built);
    assert_eq!(n.unavailable, failing);
    assert_eq!(n.downloads, built - failing);
    assert_eq!(n.dashes, m.apps.len() * COLUMNS.len() - built);
    assert_eq!(n.web_rows, m.apps.iter().filter(|a| a.docker).count());
}

#[test]
fn the_numbers_of_a_small_manifest_are_the_ones_one_counts_by_hand() {
    let mut desktop_only = made("AzDesk", &[Platform::Linux, Platform::Windows]);
    desktop_only.docker = true;
    let phone = with_known_failing(
        made("AzPhone", &[Platform::Android, Platform::Ios]),
        "ios",
        "no bin yet",
    );
    let both = made("AzBoth", &[Platform::MacosArm64, Platform::Android]);
    let m = manifest_of(vec![desktop_only, phone, both]);
    let n = numbers(&m);
    // AzDesk: 2 files; AzPhone: 4 files, 2 of them failing; AzBoth: 3 files
    assert_eq!(
        n,
        Numbers {
            apps: 3,
            built_files: 9,
            downloads: 7,
            unavailable: 2,
            dashes: 3 * 8 - 9,
            web_rows: 1
        }
    );
    assert_eq!(n.links(), 8);
    assert_eq!(section(&m).matches("<a ").count(), 8);
    assert!(
        n.to_string()
            .contains("3 apps x 8 columns: 9 built files = 7 downloads + 2 not available"),
        "{n}"
    );
}

// ---------------------------------------------------------------------------------------------
// The wording
// ---------------------------------------------------------------------------------------------

#[test]
fn the_page_calls_the_demos_self_contained() {
    let html = section(&manifest());
    assert!(html.contains("self-contained"), "{}", intro_html());
    assert!(
        html.contains("linked statically against the prebuilt libazul"),
        "{}",
        intro_html()
    );
    for old in [
        "one dll, many small binaries",
        "instead of bundling",
        "every app links the same one library",
    ] {
        assert!(!html.contains(old), "the page still says {old:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// Everything below reads the repository (and needs the toml crate)
// ---------------------------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("doc/ has a parent")
        .to_path_buf()
}

fn cargo_toml(path: &Path) -> toml::Table {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::from_str::<toml::Table>(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn package_name(manifest: &toml::Table) -> Option<&str> {
    manifest.get("package")?.get("name")?.as_str()
}

/// The `name` of every `[[bin]]` a Cargo.toml declares.
fn bin_names(manifest: &toml::Table) -> Vec<String> {
    manifest
        .get("bin")
        .and_then(|b| b.as_array())
        .map(|bins| {
            bins.iter()
                .filter_map(|b| b.get("name")?.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

/// {package: directory} of the Az packages the workspace lists (the members of the root Cargo.toml).
fn workspace_az_packages() -> BTreeMap<String, String> {
    let root = repo_root();
    let workspace = cargo_toml(&root.join("Cargo.toml"));
    let members: Vec<String> = workspace
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .expect("the root Cargo.toml has [workspace] members")
        .iter()
        .filter_map(|m| m.as_str().map(String::from))
        .collect();
    let mut found = BTreeMap::new();
    for member in members {
        let path = root.join(&member).join("Cargo.toml");
        if !path.is_file() {
            continue;
        }
        if let Some(name) = package_name(&cargo_toml(&path)) {
            if is_az_name(name) {
                found.insert(name.to_string(), member);
            }
        }
    }
    found
}

#[test]
fn the_golden_names_are_the_ones_scripts_test_demo_apps_py_pins_against_the_script() {
    let py = std::fs::read_to_string(repo_root().join("scripts/test_demo_apps.py"))
        .expect("scripts/test_demo_apps.py is readable");
    for line in GOLDEN_AZWIDGETS.lines().chain(GOLDEN_AZCOMPANION.lines()) {
        let escaped = line.replace('\t', "\\t");
        assert!(
            py.contains(&escaped),
            "scripts/test_demo_apps.py does not hold the line {escaped:?}: the golden lines of this test and of that one have to stay the same, \
             because the Rust names and the script's names are only equal as long as both match them"
        );
    }
}

#[test]
fn the_compiled_in_manifest_is_the_file_of_the_repository() {
    let text = std::fs::read_to_string(repo_root().join("scripts/demo_apps.toml"))
        .expect("the manifest is readable");
    assert_eq!(MANIFEST_TOML, text);
    assert_eq!(
        Manifest::parse(&text).expect("it parses").apps.len(),
        manifest().apps.len()
    );
}

#[test]
fn a_key_the_reader_does_not_know_is_ignored_and_a_platform_it_does_not_know_is_an_error() {
    let text = "schema = 1\n\
        [lib.desktop]\nfeatures = \"dist\"\n\
        [[app]]\npackage = \"AzA\"\ndir = \"examples/a\"\ndescription = \"d\"\nplatforms = [\"linux\", \"android\"]\n\
        bundle_id = \"com.x\"\nweight = 3\nsomething_new = [1, 2]\n\
        [app.android]\npermissions = [\"p\"]\n\
        [app.known_failing]\nandroid = \"why\"\n\
        [[app]]\npackage = \"AzB\"\ndir = \"examples/b\"\nname = \"The B\"\ndescription = \"d\"\nplatforms = [\"ios\"]\n\
        docker = true\nneeds = [\"x\", \"y\"]\nmobile_lib = \"mobile\"\n";
    let m = Manifest::parse(text).expect("the manifest parses");
    assert_eq!(m.schema, 1);
    assert_eq!(m.apps.len(), 2);
    let a = &m.apps[0];
    assert_eq!(
        (a.package.as_str(), a.dir.as_str(), a.weight, a.docker),
        ("AzA", "examples/a", 3, false)
    );
    assert_eq!(a.platforms, [Platform::Linux, Platform::Android]);
    assert_eq!(
        a.known_failing.get("android").map(String::as_str),
        Some("why")
    );
    assert_eq!(a.display_name(), "AzA");
    let b = &m.apps[1];
    assert_eq!((b.display_name(), b.weight, b.docker), ("The B", 1, true));
    assert_eq!(b.needs, ["x", "y"]);
    assert_eq!(b.mobile_lib.as_deref(), Some("mobile"));
    assert_eq!(m.problems(), Vec::<String>::new());

    let bad = text.replace("\"ios\"", "\"amiga\"");
    let err = Manifest::parse(&bad).expect_err("an unknown platform is not read as nothing");
    assert!(err.contains("amiga"), "{err}");
    assert!(Manifest::parse("schema = ").is_err());
}

#[test]
fn every_az_package_of_the_workspace_is_an_app_and_every_app_a_package() {
    let m = manifest();
    let packages = workspace_az_packages();
    let listed: BTreeSet<&str> = m.apps.iter().map(|a| a.package.as_str()).collect();
    let mut problems = Vec::new();
    for (package, dir) in &packages {
        if !listed.contains(package.as_str()) {
            problems.push(format!(
                "{package} is a package of the workspace ({dir}) but not in scripts/demo_apps.toml: add it with its platforms (an app that does not belong on the page says why in `known_failing`)"
            ));
        }
    }
    for app in &m.apps {
        match packages.get(&app.package) {
            None => problems.push(format!(
                "{} is in scripts/demo_apps.toml but is no AzXxx package of the workspace",
                app.package
            )),
            Some(dir) if *dir != app.dir => problems.push(format!(
                "{}: dir is {:?}, the workspace has it at {dir:?}",
                app.package, app.dir
            )),
            Some(_) => {}
        }
    }
    println!(
        "{} Az packages in the workspace, {} apps in the manifest",
        packages.len(),
        m.apps.len()
    );
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    assert_eq!(packages.len(), m.apps.len());
}

#[test]
fn package_binary_and_asset_stem_are_one_string() {
    let m = manifest();
    let root = repo_root();
    let mut problems = Vec::new();
    let mut pending = Vec::new();
    for app in &m.apps {
        let p = &app.package;
        let cargo = cargo_toml(&root.join(&app.dir).join("Cargo.toml"));
        if package_name(&cargo) != Some(p.as_str()) {
            problems.push(format!(
                "{}/Cargo.toml names the package {:?}, the manifest {p:?}",
                app.dir,
                package_name(&cargo)
            ));
        }
        let bins = bin_names(&cargo);
        if !bins.is_empty() && !bins.contains(p) {
            // an app whose build is a known failure says why in the manifest, and is not held to what it lacks
            if app.known_failing.is_empty() {
                problems.push(format!("{p} declares [[bin]] {bins:?} and none is named {p}: the staged file and the page would not agree"));
            } else {
                pending.push(p.as_str());
            }
        }
        for column in columns_of(app) {
            let stem = format!("{p}-");
            if !staged_name(app, column).starts_with(&stem)
                || !release_name(app, column).starts_with(&stem)
            {
                problems.push(format!(
                    "{p}: the file of {} does not start with the package name",
                    column.id()
                ));
            }
        }
    }
    println!("apps held to their binary name: {}; exempt while their build is a known failure: {pending:?}", m.apps.len() - pending.len());
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_docker_app_has_a_dockerfile_and_only_they_have_one() {
    let m = manifest();
    let root = repo_root();
    for app in &m.apps {
        let has = root.join(&app.dir).join("Dockerfile").is_file();
        assert_eq!(
            has,
            app.docker,
            "{}: docker = {} in the manifest but {}/Dockerfile {}",
            app.package,
            app.docker,
            app.dir,
            if has { "exists" } else { "does not exist" }
        );
    }
}
