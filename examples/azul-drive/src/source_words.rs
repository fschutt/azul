//! The Add drive dialog's sources in the window's language: azul-storage's catalog (no azul
//! types; English for its tests and the command line) names its groups, sources, fields, form
//! problems and drive kinds in English. AzDrive's resources hold those words by their English
//! (`l10n::named`: `azdrive-field-server-address`), so a source the catalog adds or rewords
//! shows in English until its words are here - `source_words_tests` names the missing ones.

use azul_appkit::l10n::{self, t, t_args, Arg};
use azul_storage::catalog::{
    DriveKind, FieldKind, FieldSpec, FormProblem, ServiceGroup, ServiceSpec,
};

fn named(what: &str, english: &str) -> String {
    l10n::named("AzDrive", what, english)
}

/// A group's heading: `Netzwerk & NAS`.
#[must_use]
pub(crate) fn group(group: ServiceGroup) -> String {
    named("group", group.title())
}

/// A source's name: `Ordner auf diesem Computer`; a product's name as it is (`Dropbox`).
#[must_use]
pub(crate) fn name(spec: &ServiceSpec) -> String {
    named("service", spec.name)
}

/// A source's line under its name.
#[must_use]
pub(crate) fn summary(spec: &ServiceSpec) -> String {
    named("summary", spec.summary)
}

/// A field's label: `Serveradresse`.
#[must_use]
pub(crate) fn field_label(f: &FieldSpec) -> String {
    named("field", f.label)
}

/// A field's title over it: its label, with "(optional)" when it may stay empty.
#[must_use]
pub(crate) fn field_title(f: &FieldSpec) -> String {
    if f.required {
        field_label(f)
    } else {
        t_args(
            "azdrive-form-optional",
            &[("field", Arg::from(field_label(f)))],
        )
    }
}

/// A field's line under it (empty for none).
#[must_use]
pub(crate) fn help(f: &FieldSpec) -> String {
    if f.help.is_empty() {
        String::new()
    } else {
        named("help", f.help)
    }
}

/// What an empty field shows: words in the window's language, an example address as it is.
#[must_use]
pub(crate) fn placeholder(f: &FieldSpec) -> String {
    if f.placeholder.contains(' ') {
        named("placeholder", f.placeholder)
    } else {
        f.placeholder.to_string()
    }
}

/// What a filled form lacks, as a sentence: `„Serveradresse“ ist erforderlich.`
#[must_use]
pub(crate) fn problem(problem: FormProblem) -> String {
    let field = |f: &FieldSpec| ("field", Arg::from(field_label(f)));
    match problem {
        FormProblem::NoName => t("azdrive-add-give-name"),
        FormProblem::Required(f) => t_args("azdrive-form-required", &[field(f)]),
        FormProblem::NotAnAddress(f) => t_args(
            "azdrive-form-not-an-address",
            &[field(f), ("scheme", Arg::from(problem.scheme()))],
        ),
        FormProblem::NotANumber(f) => t_args("azdrive-form-not-a-number", &[field(f)]),
        FormProblem::NotOnOff(f) => t_args("azdrive-form-not-on-off", &[field(f)]),
        FormProblem::NotOneOf(f) => {
            let words = match f.kind {
                FieldKind::Choice(words) => words.join(", "),
                _ => String::new(),
            };
            t_args(
                "azdrive-form-not-one-of",
                &[field(f), ("words", Arg::from(words))],
            )
        }
    }
}

/// What kind of drive it is (Properties' Type, the drive's tile): `Lokaler Datenträger`.
#[must_use]
pub(crate) fn kind(kind: &DriveKind) -> String {
    match kind {
        DriveKind::LocalDisk => t("azdrive-kind-local-disk"),
        DriveKind::AzlinCloud => t("azdrive-kind-azlin"),
        DriveKind::S3Bucket => t("azdrive-kind-s3-bucket"),
        DriveKind::Source(spec) => name(spec),
        DriveKind::Scheme(scheme) => scheme.clone(),
        DriveKind::Database(engine) => t_args(
            "azdrive-kind-database",
            &[("engine", Arg::from(engine.name()))],
        ),
    }
}
