//! Algorithm 3: the member key under the recovery code, through Argon2id (planning/architecture.md
//! section 5.5 "Recovery" and "Envelope v2": `alg` 3 "Argon2id passphrase"; build unit S15, lane
//! W1-S15d).
//!
//! The key of the wrap is derived in two steps: Argon2id (version 1.3) over the code's 16 bytes with
//! the wrap's own random 16-byte salt and its cost gives a root key, and BLAKE3 `derive_key` under
//! the context `azlin 2026-11 recovery code wrap v1` turns the root into the AEAD key. The salt,
//! the memory in KiB, the passes and the lanes are the envelope's `params` (`{1: salt, 2: m_kib,
//! 3: t, 4: p}`): they are part of the associated data and of the signature, so a changed cost does
//! not open the wrap.
//!
//! THE COST COMES FROM THE SERVER and is paid before the code is known to be right, so the reader
//! checks it first: below the FLOOR the caller passes (a server that lowers the cost, or an old weak
//! wrap replayed) is refused, and above the LIMITS below (memory, passes, lanes and the work of
//! them together: the limits of today's `RecoveryKdf`) is refused before any memory is asked for.
//! Both checks run before the signature is checked and before anything is hashed. The memory
//! Argon2id works in is wiped after the derivation.
//!
//! Draws from the entropy seam, in this order: the 16-byte salt, then the 24-byte nonce.

use zeroize::{Zeroize, Zeroizing};

use super::code::RecoveryCode;
use super::error::RecoveryError;
use super::open::{malformed, open_recovery, require_method, slot_rcpt};
use crate::derive::{derive, RECOVERY_WRAP};
use crate::envelope::{
    seal_with_key, Alg, Ctx, Envelope, EnvelopeError, Header, Params, ParamsError, RecoveryMethod,
};
use crate::ids::DeviceId;
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, VerifyAt};

/// A memory size in KiB (1,024 bytes): the unit of Argon2id's memory cost.
///
/// Caller: `KdfParams`, `KdfFloor`, INT-S15-1 (the floor a client is configured with).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Kib(u32);

impl Kib {
    /// `kib` KiB.
    pub const fn new(kib: u32) -> Kib {
        Kib(kib)
    }

    /// The number of KiB.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The cost of an Argon2id derivation: memory, passes over it, and lanes.
///
/// Caller: `KdfProfile::params`, `wrap_with_code` (the cost it writes), `unwrap_with_code` (the cost
/// it reads, checked against the floor and the limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KdfParams {
    /// The memory.
    pub memory: Kib,
    /// The passes over the memory.
    pub passes: u32,
    /// The lanes (Argon2id's degree of parallelism).
    pub lanes: u32,
}

/// The cost a wrap is made with.
///
/// Caller: INT-S15-1 (`Production` for every real wrap), the tests (`Test`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KdfProfile {
    /// Argon2id with 256 MiB, 3 passes and 4 lanes (DRIFT X-DELTA-W1S15D-6: the brief says "as
    /// azul-storage's `RecoveryKdf`", whose default is 1 lane; the numbers here follow the
    /// brief's check C3).
    Production,
    /// Argon2id with 8 MiB, 1 pass and 2 lanes: fast enough for tests. Never for a real wrap: it is
    /// below the production floor.
    Test,
}

impl KdfProfile {
    /// The cost of the profile.
    ///
    /// Caller: `wrap_with_code`, the tests (the production numbers are asserted, never run).
    pub const fn params(self) -> KdfParams {
        match self {
            KdfProfile::Production => KdfParams {
                memory: Kib::new(256 * 1024),
                passes: 3,
                lanes: 4,
            },
            KdfProfile::Test => KdfParams {
                memory: Kib::new(8 * 1024),
                passes: 1,
                lanes: 2,
            },
        }
    }
}

/// The least cost a device accepts when it unwraps: an envelope below it is refused as
/// [`RecoveryError::BelowFloor`] before anything is hashed.
///
/// Caller: INT-S15-1 (`KdfFloor::PRODUCTION` for every real unwrap), the tests (`KdfFloor::TEST`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KdfFloor {
    memory: Kib,
    passes: u32,
}

impl KdfFloor {
    /// 64 MiB and 2 passes: a quarter of the production cost (a server-lowered cost is refused).
    pub const PRODUCTION: KdfFloor = KdfFloor::new(Kib::new(64 * 1024), 2);

    /// 8 MiB and 1 pass: what `KdfProfile::Test` writes.
    pub const TEST: KdfFloor = KdfFloor::new(Kib::new(8 * 1024), 1);

    /// A floor of this memory and these passes.
    pub const fn new(memory: Kib, passes: u32) -> KdfFloor {
        KdfFloor { memory, passes }
    }

    /// The least memory.
    pub const fn memory(&self) -> Kib {
        self.memory
    }

    /// The fewest passes.
    pub const fn passes(&self) -> u32 {
        self.passes
    }
}

/// The most memory a device derives with: 1 GiB (the limit of today's `RecoveryKdf`).
///
/// Caller: `check_cost`.
pub const MAX_KDF_MEMORY: Kib = Kib::new(1024 * 1024);

/// The most passes a device derives with: 8.
///
/// Caller: `check_cost`.
pub const MAX_KDF_PASSES: u32 = 8;

/// The most lanes a device derives with: 8.
///
/// Caller: `check_cost`.
pub const MAX_KDF_LANES: u32 = 8;

/// The most work a device derives with: KiB of memory times passes, 4 GiB-passes (a request of
/// 4 GiB and 64 passes used to be accepted by today's code: an abort on a small device, minutes of
/// CPU on a large one).
///
/// Caller: `check_cost`.
pub const MAX_KDF_WORK: u64 = 4 * 1024 * 1024;

/// Checks a cost: above the limits is `AboveLimit` (checked first: it is what must never be
/// allocated), below the floor is `BelowFloor` (DRIFT X-DELTA-W1S15D-16: the limits are today's).
pub(crate) fn check_cost(params: &KdfParams, floor: &KdfFloor) -> Result<(), RecoveryError> {
    check_limits(params)?;
    if params.memory < floor.memory || params.passes < floor.passes {
        return Err(RecoveryError::BelowFloor);
    }
    Ok(())
}

/// Checks a cost against the limits only.
pub(crate) fn check_limits(params: &KdfParams) -> Result<(), RecoveryError> {
    let work = u64::from(params.memory.get()) * u64::from(params.passes);
    if params.memory > MAX_KDF_MEMORY
        || params.passes > MAX_KDF_PASSES
        || params.lanes > MAX_KDF_LANES
        || work > MAX_KDF_WORK
    {
        return Err(RecoveryError::AboveLimit);
    }
    Ok(())
}

/// The root key of `code`: Argon2id (version 1.3) over the code's bytes, the salt and the cost,
/// 32 bytes. The memory is taken fallibly (a device that cannot give it gets `Kdf`, not an abort)
/// and wiped afterwards.
fn derive_root(
    code: &RecoveryCode,
    salt: &[u8; 16],
    params: &KdfParams,
) -> Result<Secret<32>, RecoveryError> {
    let argon_params =
        argon2::Params::new(params.memory.get(), params.passes, params.lanes, Some(32))
            .map_err(|_| RecoveryError::Kdf)?;
    let blocks = argon_params.block_count();
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon_params,
    );
    let mut memory: Vec<argon2::Block> = Vec::new();
    memory
        .try_reserve_exact(blocks)
        .map_err(|_| RecoveryError::Kdf)?;
    memory.resize(blocks, argon2::Block::default());
    let mut root = Secret::<32>::from_bytes([0u8; 32]);
    let outcome = argon.hash_password_into_with_memory(
        code.as_secret().expose(),
        salt,
        root.expose_mut().as_mut_slice(),
        &mut memory,
    );
    memory.zeroize();
    outcome.map_err(|_| RecoveryError::Kdf)?;
    Ok(root)
}

/// The AEAD key of an algorithm 3 wrap: BLAKE3 `derive_key` under the context
/// `azlin 2026-11 recovery code wrap v1` over the Argon2id root.
fn code_key(
    code: &RecoveryCode,
    salt: &[u8; 16],
    params: &KdfParams,
) -> Result<Secret<32>, RecoveryError> {
    let root = derive_root(code, salt, params)?;
    Ok(derive(&RECOVERY_WRAP, &root))
}

/// The salt and the cost an algorithm 3 envelope names.
fn argon_params_of(env: &Envelope) -> Result<([u8; 16], KdfParams), RecoveryError> {
    match &env.params {
        Params::Argon2id { salt, m_kib, t, p } => Ok((
            *salt,
            KdfParams {
                memory: Kib::new(*m_kib),
                passes: *t,
                lanes: *p,
            },
        )),
        _ => Err(malformed(EnvelopeError::BadParams(ParamsError::WrongKind))),
    }
}

/// Wraps `member_key` (the plaintext of the member key bundle: the seeds of S15c1's `KeyBundle`)
/// under the recovery code as an algorithm 3 envelope for the context `ctx`
/// (`recovery/<member>/code`), signed by the device `signer` with `sk`.
///
/// The cost is `profile`'s. The draws are the 16-byte salt and the 24-byte nonce. An entropy
/// failure, a context that is not the code's, a plaintext over the limit each return an error and
/// NO envelope. Only the code opens the wrap again; a Shamir share never wraps the member key (it
/// wraps a share of the CODE: the chain is shares to code to member key).
///
/// Caller: INT-S15-1 (account setup, and whenever the member key or the recovery code changes).
pub fn wrap_with_code<const N: usize>(
    member_key: &Secret<N>,
    code: &RecoveryCode,
    ctx: &Ctx,
    profile: KdfProfile,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    wrap_code_with_params(
        member_key.expose(),
        code,
        ctx,
        profile.params(),
        signer,
        sk,
        ent,
    )
}

/// `wrap_with_code` for any cost within the limits: the one function that makes an algorithm 3
/// envelope. The tests make weak wraps (below every floor) with it.
pub(crate) fn wrap_code_with_params(
    plaintext: &[u8],
    code: &RecoveryCode,
    ctx: &Ctx,
    params: KdfParams,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    require_method(ctx, RecoveryMethod::Code)?;
    check_limits(&params)?;
    let mut salt = [0u8; 16];
    ent.fill(&mut salt)?;
    let key = code_key(code, &salt, &params)?;
    let header = Header {
        alg: Alg::Argon2id,
        rcpt: slot_rcpt(Alg::Argon2id, ctx, &[]),
        epk: None,
        kem_ct: None,
        params: Params::Argon2id {
            salt,
            m_kib: params.memory.get(),
            t: params.passes,
            p: params.lanes,
        },
        ctx: ctx.clone(),
        signer: *signer,
    };
    seal_with_key(&header, &key, plaintext, sk, ent).map_err(RecoveryError::from)
}

/// Opens an algorithm 3 envelope with the recovery code: the plaintext (the member key), in a
/// buffer that is wiped when dropped.
///
/// The checks run in this order, and each failure stops the call with its own error: the format,
/// the algorithm (`WrongAlg`; 6 and 7 are `Unsupported`), that the context is `expect` and is the
/// code's (`Open(CtxMismatch)`, `WrongCtx`), the cost against the limits (`AboveLimit`) and the
/// `floor` (`BelowFloor`), the signature of the wrapping device through `dir` at `at`, and only
/// then Argon2id and the cipher (`Auth` for a wrong code or a changed header, parameter or
/// ciphertext). Nothing is hashed, and no memory is asked for, before the signature has verified.
///
/// `at` is where the wrapping device's key is asked for. A recovery passes the point of the
/// WRITE of the wrap (the position or time of the registry's row), not `VerifyAt::Now`: the
/// device that wrapped the key may have been revoked since, and a lost laptop is exactly when a
/// recovery is needed.
///
/// Caller: INT-S15-1 (the recovery screen: the code, typed or recombined from shares).
pub fn unwrap_with_code(
    env: &Envelope,
    code: &RecoveryCode,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
    floor: KdfFloor,
) -> Result<Zeroizing<Vec<u8>>, RecoveryError> {
    unwrap_code_with(env, expect, dir, at, floor, |salt, params| {
        code_key(code, salt, params)
    })
}

/// `unwrap_with_code` with the key derivation as a parameter: the tests count its calls to show
/// that it runs after every other check. The production caller passes the real Argon2id.
pub(crate) fn unwrap_code_with(
    env: &Envelope,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
    floor: KdfFloor,
    key_of: impl FnOnce(&[u8; 16], &KdfParams) -> Result<Secret<32>, RecoveryError>,
) -> Result<Zeroizing<Vec<u8>>, RecoveryError> {
    open_recovery(
        env,
        RecoveryMethod::Code,
        expect,
        dir,
        at,
        |env| {
            let (_, params) = argon_params_of(env)?;
            check_cost(&params, &floor)
        },
        |env| {
            let (salt, params) = argon_params_of(env)?;
            key_of(&salt, &params)
        },
    )
}
