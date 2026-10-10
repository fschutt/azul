//! An encrypted drive's recovery methods in AzDrive (feature `encryption`; D51, milestone C14).
//!
//! The EMERGENCY KIT: the recovery code on paper - a short explanation, the drive's name (never
//! its id or its bucket), the code in groups and as a QR code (azul-appkit's `qr`) - made as a
//! PDF by azul's PDF writer from a DOM laid out for A4 paper. The recovery sheet offers it
//! three ways: Print (the PDF opens in the system's viewer from a private copy in the run's
//! cache folder, deleted when the sheet closes and at the next start), Save as PDF (the
//! system's save dialog) and Save to a USB stick (a folder picked, the PDF written into it).

use azul_storage::crypto::Zeroizing;

/// The kit's explanation, in its order.
pub(crate) const KIT_TEXT: [&str; 0] = [];

/// What the kit shows. The drive's name only: a name that is (or holds) the drive's id or its
/// bucket shows as "your Azlin drive".
pub(crate) struct Kit {
    pub drive_name: String,
    /// The recovery code as the sheet shows it. A secret: never printed.
    pub code: Zeroizing<String>,
    /// The day it was made (`2026-10-10`).
    pub made: String,
}

impl Kit {
    /// The kit of `code` for the drive called `drive_name`; `hidden` are the drive's id, its
    /// Azlin id and its bucket, which no kit shows.
    pub(crate) fn new(
        drive_name: &str,
        _hidden: &[&str],
        code: Zeroizing<String>,
        made: String,
    ) -> Kit {
        Kit {
            drive_name: drive_name.to_string(),
            code,
            made,
        }
    }

    /// The kit's file name: `Azlin Emergency Kit - <name>.pdf`.
    pub(crate) fn file_name(&self) -> String {
        String::new()
    }

    /// Every line of the kit's text but the code.
    pub(crate) fn lines(&self) -> Vec<String> {
        Vec::new()
    }

    /// The text its QR code holds: the code as the sheet shows it, which "Unlock with the
    /// recovery code" takes as it is.
    pub(crate) fn qr_text(&self) -> Zeroizing<String> {
        Zeroizing::new(String::new())
    }
}

#[cfg(test)]
mod tests {
    use azul_appkit::qr::QrCode;
    use azul_storage::crypto::keys::RecoveryCode;

    use super::*;

    fn code() -> Zeroizing<String> {
        RecoveryCode::from_bytes([0x5A; 16]).to_text()
    }

    #[test]
    fn the_kit_explains_names_the_drive_and_holds_the_code_as_text_and_qr() {
        let kit = Kit::new(
            "Photos",
            &["d_7k2m", "d-7k2m"],
            code(),
            String::from("2026-10-10"),
        );
        let text = kit.lines().join("\n");
        assert!(
            text.contains("This is the only key to your files"),
            "{text}"
        );
        assert!(text.contains("Azlin can't reset it"), "{text}");
        assert!(text.contains("\"Photos\""), "the drive's name: {text}");
        assert!(text.contains("2026-10-10"), "the day it was made: {text}");
        assert!(text.contains("48 hours"), "what a recovery costs: {text}");
        assert!(
            !text.contains(code().as_str()),
            "the code is not one of the lines"
        );
        assert_eq!(kit.qr_text().as_str(), code().as_str());
        assert!(
            RecoveryCode::parse(&kit.qr_text()).is_some(),
            "the QR's text unlocks"
        );
        assert_eq!(
            QrCode::encode(kit.qr_text().as_bytes()).unwrap().version(),
            3
        );
        assert!(KIT_TEXT.len() >= 3);
    }

    #[test]
    fn the_kit_never_shows_the_drives_id_or_its_bucket() {
        for name in ["d_7k2m", "Photos (d-7k2m)", "  d_7k2m  ", "D_7K2M"] {
            let kit = Kit::new(
                name,
                &["d_7k2m", "d-7k2m"],
                code(),
                String::from("2026-10-10"),
            );
            let text = format!("{}\n{}", kit.lines().join("\n"), kit.file_name());
            assert!(!text.to_lowercase().contains("7k2m"), "{name}: {text}");
            assert!(text.contains("your Azlin drive"), "{name}: {text}");
        }
    }

    #[test]
    fn the_kits_file_name_is_a_safe_name_of_the_drive() {
        let kit = Kit::new(
            "Photos/2026: ok?",
            &["d_1"],
            code(),
            String::from("2026-10-10"),
        );
        assert_eq!(kit.file_name(), "Azlin Emergency Kit - Photos-2026-ok.pdf");
        let kit = Kit::new("???", &["d_1"], code(), String::from("2026-10-10"));
        assert_eq!(kit.file_name(), "Azlin Emergency Kit.pdf");
    }
}
