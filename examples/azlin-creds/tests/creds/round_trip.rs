//! S9E-01 `a_credential_round_trips`: certify a credential key under the identity key, issue claims,
//! verify under the anchors: `Ok(VerifiedCredential)` with every claim equal to the issued one (one
//! assertion per getter); and the wire form is stable: encode, decode, encode gives the same bytes.

use azlin_creds::{
    verify_credential, verify_credential_bytes, AppPerms, AppScope, Credential, CredentialIssuer,
    LinkRef, Scope, Scopes, SsoClaim,
};
use azlin_regbase::{SmallText, SpaceId};

use crate::support::*;

#[test]
fn s9e_01_a_device_credential_round_trips() {
    let (f, credential) = issue_fixture();
    let verified = verify_credential(&credential, &f.anchors, &placement(), now(ISSUED), &f.rules)
        .expect("the credential verifies");
    let g = grant();
    assert_eq!(verified.account(), g.account);
    assert_eq!(verified.member(), g.member);
    assert_eq!(verified.device(), g.device);
    assert_eq!(verified.family(), g.family);
    assert_eq!(verified.policy_epoch(), g.policy_epoch);
    assert!(verified.scopes().is_none(), "a device credential has none");
    assert_eq!(verified.issued_at(), g.issued_at);
    assert_eq!(verified.expires_at(), g.expires_at);
    assert_eq!(verified.key_epoch(), EPOCH, "stamped from the certificate");
    assert_eq!(verified.nonce(), g.nonce);
    assert!(verified.sso().is_none());
    assert_eq!(verified.group(), GROUP);
    assert_eq!(verified.claims(), credential.claims());
}

#[test]
fn s9e_01_an_app_principals_credential_round_trips_with_scopes_and_sso() {
    let f = fixture();
    let scopes = Scopes::new(vec![
        Scope::App(AppScope {
            space: SpaceId::from_bytes([9; 16]),
            perms: AppPerms::READ,
        }),
        Scope::LinkRead {
            link: LinkRef::from_bytes([1; 16]),
        },
    ])
    .expect("two scopes");
    let sso = SsoClaim {
        provider: SmallText::new("entra-contoso").expect("a provider"),
        auth_time: at(ISSUED - 60),
    };
    let mut g = grant();
    g.device = None;
    g.family = None;
    g.scopes = Some(scopes.clone());
    g.sso = Some(sso.clone());
    let issuer = CredentialIssuer::new(&f.cred, f.cert, &f.anchors, at(ISSUED)).expect("issuer");
    let credential = issuer.issue(g.clone()).expect("an app credential");
    let verified = verify_credential(&credential, &f.anchors, &placement(), now(ISSUED), &f.rules)
        .expect("verifies");
    assert_eq!(verified.device(), None);
    assert_eq!(verified.family(), None);
    assert_eq!(verified.scopes(), Some(&scopes));
    assert_eq!(verified.sso(), Some(&sso));
    assert_eq!(verified.nonce(), g.nonce);
}

#[test]
fn s9e_01_the_wire_form_round_trips_byte_for_byte() {
    let (f, credential) = issue_fixture();
    let bytes = credential.encode().expect("encodes");
    let again = Credential::decode(&bytes).expect("decodes");
    assert_eq!(again, credential);
    assert_eq!(
        again.encode().expect("encodes again"),
        bytes,
        "a credential read from bytes writes the same bytes"
    );
    let verified = verify_credential_bytes(&bytes, &f.anchors, &placement(), now(ISSUED), &f.rules)
        .expect("the bytes verify");
    assert_eq!(verified.account(), grant().account);
}

#[test]
fn s9e_01_a_certificate_round_trips_through_its_own_bytes() {
    let f = fixture();
    let bytes = f.cert.encode().expect("encodes");
    assert_eq!(azlin_creds::Certificate::decode(&bytes), Ok(f.cert));
    azlin_creds::verify_cert(&f.cert, &f.anchors).expect("the fixture's certificate verifies");
}
