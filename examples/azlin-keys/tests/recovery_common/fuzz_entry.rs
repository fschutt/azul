//! The entry function of the fuzz target `fuzz/fuzz_targets/contact_text.rs`, which
//! `tests/recovery_hygiene.rs` also runs on 20,000 seeded inputs and their bit flips: a panic is a
//! failed test here and an abort in the shipped profile (planning/architecture.md section 1.4
//! rule 3: every parser has a fuzz target). The file is self-contained (it names only `azlin_keys`
//! and std) because the fuzz target includes it by `#[path]`.
//!
//! The same bytes are tried as a text (the recovery code, the contact key, the request key, the
//! sealed share, the reply, the printed share), as the bytes of up to four shares (`combine`) and as
//! an envelope (the three unwraps and the share open, under the Test floor).
//!
//! The oracles: nothing panics; every text that parses prints back to a text that parses to the same
//! value; a recombination that succeeds has the check value its shares carry.

use std::sync::OnceLock;

use azlin_keys::envelope::{Ctx, Envelope, RecipientSecret, RecoveryMethod};
use azlin_keys::recovery::{
    combine, open_share, prf_inputs, printed_share_from_text, printed_share_text, reply_from_text,
    reply_text, share_from_text, share_text, unwrap_with_code, unwrap_with_prf, ContactKey,
    KdfFloor, RecoveryCode, RequestKey, Share,
};
use azlin_keys::{
    DeviceDirectory, DeviceId, DirectoryError, MemberId, Secret, SignKeypair, VerifyAt, VerifyKey,
};

/// A directory that vouches for one key for every device.
struct AnyDevice(VerifyKey);

impl DeviceDirectory for AnyDevice {
    fn verify_key(&self, _device: &DeviceId, _at: VerifyAt) -> Result<VerifyKey, DirectoryError> {
        Ok(self.0)
    }
}

struct Fixtures {
    dir: AnyDevice,
    code: RecoveryCode,
    prf_output: Secret<32>,
    contact: RecipientSecret,
    code_ctx: Ctx,
    prf_ctx: Ctx,
    share_ctx: Ctx,
}

static FIXTURES: OnceLock<Fixtures> = OnceLock::new();

fn fixtures() -> &'static Fixtures {
    FIXTURES.get_or_init(|| {
        let member = MemberId::from_bytes([0x71; 16]);
        Fixtures {
            // The signer of the lane's known-answer vectors: an unmodified vector reaches the
            // key derivation, every changed one stops at the signature or before.
            dir: AnyDevice(SignKeypair::from_seed(&Secret::from_bytes(seed(0x90))).verify_key()),
            code: RecoveryCode::from_secret(Secret::from_bytes(array16(0x10))),
            prf_output: Secret::from_bytes(seed(0x40)),
            contact: RecipientSecret::new(&Secret::from_bytes(seed(0x20)), None),
            code_ctx: Ctx::recovery(&member, RecoveryMethod::Code),
            prf_ctx: Ctx::recovery(&member, RecoveryMethod::Prf),
            share_ctx: Ctx::recovery(&member, RecoveryMethod::Share),
        }
    })
}

fn seed(start: u8) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = start.wrapping_add(i as u8);
    }
    out
}

fn array16(start: u8) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = start.wrapping_add(i as u8);
    }
    out
}

/// Every text parser on `text`: what parses prints back to a text that parses to the same value.
fn text_parsers(text: &str) {
    if let Ok(code) = RecoveryCode::parse(text) {
        let again = RecoveryCode::parse(&code.to_text()).expect("a printed code parses");
        assert!(code.ct_eq(&again), "a code does not print back");
    }
    if let Ok(key) = ContactKey::from_text(text) {
        assert_eq!(ContactKey::from_text(&key.to_text()), Ok(key));
    }
    if let Ok(key) = RequestKey::from_text(text) {
        assert_eq!(RequestKey::from_text(&key.to_text()), Ok(key));
    }
    if let Ok(env) = share_from_text(text) {
        let printed = share_text(&env).expect("a parsed share prints");
        assert_eq!(share_from_text(&printed), Ok(env));
    }
    if let Ok(env) = reply_from_text(text) {
        let printed = reply_text(&env).expect("a parsed reply prints");
        assert_eq!(reply_from_text(&printed), Ok(env));
    }
    if let Ok(share) = printed_share_from_text(text) {
        let printed = printed_share_text(&share);
        let again = printed_share_from_text(&printed).expect("a printed share parses");
        assert_eq!(
            (again.x, again.k, again.n, again.split_id),
            (share.x, share.k, share.n, share.split_id)
        );
        assert!(again.y.ct_eq(&share.y));
    }
}

/// Up to four shares out of the bytes (23 bytes each: x, k, n, split id, y) through `combine`.
fn shares_from_bytes(data: &[u8]) {
    let shares: Vec<Share> = data
        .as_chunks::<23>()
        .0
        .iter()
        .take(4)
        .map(|chunk| {
            let mut split_id = [0u8; 4];
            split_id.copy_from_slice(&chunk[3..7]);
            let mut y = [0u8; 16];
            y.copy_from_slice(&chunk[7..23]);
            Share {
                x: chunk[0],
                k: chunk[1],
                n: chunk[2],
                y: Secret::from_bytes(y),
                split_id,
            }
        })
        .collect();
    if let Ok(secret) = combine(&shares) {
        let code = RecoveryCode::from_secret(secret);
        assert_eq!(
            Some(code.check_value()),
            shares.first().map(|share| share.split_id),
            "a recombination that succeeds has the check value its shares carry"
        );
    }
    for share in &shares {
        let _ = printed_share_text(share);
    }
}

/// The bytes as an envelope through the three unwraps and the share open.
fn envelopes_from_bytes(data: &[u8]) {
    let Ok(env) = Envelope::from_cbor(data) else {
        return;
    };
    let fx = fixtures();
    let _ = prf_inputs(&env);
    let _ = unwrap_with_code(
        &env,
        &fx.code,
        &fx.code_ctx,
        &fx.dir,
        VerifyAt::Now,
        KdfFloor::TEST,
    );
    let _ = unwrap_with_prf(&env, &fx.prf_output, &fx.prf_ctx, &fx.dir, VerifyAt::Now);
    let _ = open_share(&env, &fx.contact, &fx.share_ctx, &fx.dir, VerifyAt::Now);
    let _ = share_text(&env);
    let _ = reply_text(&env);
}

/// The text parsers, `combine` and the unwraps on arbitrary bytes.
pub fn contact_text_entry(data: &[u8]) {
    text_parsers(&String::from_utf8_lossy(data));
    shares_from_bytes(data);
    envelopes_from_bytes(data);
}
