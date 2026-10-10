//! Trusted contacts in AzDrive (feature `encryption`; D51): the recovery code of a drive split
//! 2-of-3 to three people (azul-storage's `crypto::contacts`), and both sides of it.
//!
//! - THE OWNER, Options > Drives > the drive's recovery: "Trusted contacts: Add..." - the code
//!   typed from the kit (checked against the drive's recovery key), three people, each with the
//!   contact key their AzDrive showed them or none (then their share is printed). The shares
//!   made: a sealed one to copy and send by any channel, a printed one to print, save as PDF
//!   or put on a USB stick (a page like the kit). A share handed over counts.
//! - A CONTACT, Options > Drives > "Shares you hold for others": "Be someone's trusted contact"
//!   makes a key for that person (kept in the keyring) and shows it to send; "Take a share"
//!   keeps a sealed share that came (in the settings, sealed; the key opens it); "Help with a
//!   recovery" takes a request, shows its SAFETY NUMBER to compare with the owner in person or
//!   by phone - against scams that fake a friend - and only then answers with the share sealed
//!   to the request.
//! - THE OWNER ON A COMPUTER THAT LOST THE DRIVE, the drive's menu: "Recover with trusted
//!   contacts..." - a request (its key kept in the keyring until the recovery is done) and its
//!   safety number to read to the contacts, then two shares (their replies, or printed shares
//!   typed in). They give back the recovery code, which signs the same lockdown as "Lock down
//!   with the recovery code": the token server holds it 48 hours for the owner's devices to
//!   cancel (D42). The code shows with the kit's buttons; after the 48 hours "Unlock with the
//!   recovery code" opens the drive with it.
//!
//! On stdout, for scripts (never a secret): `AZDRIVE_CONTACTS_SHARED <drive id> <n>` (shares
//! made), `AZDRIVE_CONTACT_KEY` (a key made), `AZDRIVE_SHARE_TAKEN <n>`, `AZDRIVE_SHARE_ANSWERED`,
//! `AZDRIVE_CONTACTS_REQUEST <drive id>`, `AZDRIVE_CONTACTS_RECOVERED <drive id>`.

use azul_storage::crypto::{
    contacts::{contact_text, seal_reply, CodeShare},
    keys::{MemberPublic, MemberSecret, RecoveryCode},
};

/// One of the three people of the owner's "Add".
#[derive(Default)]
pub(crate) struct Person {
    pub name: String,
    /// Their contact key's text; empty: their share is printed.
    pub key: String,
}

/// What the owner's three people are checked into: each one's contact key (`None`: printed).
pub(crate) fn plan_shares(_people: &[Person]) -> Result<Vec<Option<MemberPublic>>, String> {
    Err(String::new())
}

/// The recovery code two shares give back.
pub(crate) fn recovered_code(
    _texts: &[&str],
    _request: &MemberSecret,
) -> Result<RecoveryCode, String> {
    let _ = (contact_text, seal_reply, CodeShare::split);
    Err(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str, key: &str) -> Person {
        Person {
            name: name.to_string(),
            key: key.to_string(),
        }
    }

    #[test]
    fn the_three_people_are_named_and_each_key_is_a_contact_key_of_its_own() {
        let ada = MemberSecret::generate().unwrap().public();
        let grace = MemberSecret::generate().unwrap().public();
        let people = [
            person("Ada", &contact_text(&ada)),
            person("Grace", &format!("  {}  ", contact_text(&grace))),
            person("Linus", ""),
        ];
        assert_eq!(
            plan_shares(&people).unwrap(),
            vec![Some(ada), Some(grace), None],
            "Linus's share is printed"
        );
        let unnamed = [person("Ada", ""), person(" ", ""), person("Linus", "")];
        assert!(plan_shares(&unnamed).unwrap_err().contains("Person 2"));
        let bad = [
            person("Ada", "azlin-contact:12"),
            person("B", ""),
            person("C", ""),
        ];
        assert!(plan_shares(&bad).unwrap_err().contains("Ada"));
        let twice = [
            person("Ada", &contact_text(&ada)),
            person("Grace", &contact_text(&ada)),
            person("Linus", ""),
        ];
        assert!(plan_shares(&twice).unwrap_err().contains("Grace"));
    }

    #[test]
    fn two_shares_give_back_the_code_whose_recovery_key_signs_the_same_lockdown() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let shares = CodeShare::split(&code).unwrap();
        let request = MemberSecret::generate().unwrap();
        let reply = seal_reply(&shares[0], &request.public()).unwrap();
        let printed = shares[2].to_text();
        let back = recovered_code(&[reply.as_str(), "", printed.as_str()], &request).unwrap();
        assert_eq!(back.as_bytes(), code.as_bytes());
        // The token server's check of the lockdown: the recovery key of the code it gave.
        assert_eq!(
            crate::encryption::recovery_key_of(&back, "d_1").public_base64(),
            crate::encryption::recovery_key_of(&code, "d_1").public_base64()
        );
        assert!(recovered_code(&[reply.as_str()], &request)
            .unwrap_err()
            .contains("Two shares"));
        let other = CodeShare::split(&RecoveryCode::from_bytes([0x11; 16])).unwrap();
        assert!(
            recovered_code(&[reply.as_str(), other[1].to_text().as_str()], &request)
                .unwrap_err()
                .contains("do not give a code back")
        );
    }
}
