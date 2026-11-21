//! The refresh frames: what a client sends to renew its credentials and what the node answers
//! (planning/architecture.md section 2.4 "refresh: device key or token -> short-lived credentials",
//! section 5.3 "Devices use rotating families. **App principals never do** ... They authenticate with a
//! non-rotating, scoped, expiring key pair instead", DRIFT X-D-07: a device whose token is lost refreshes
//! with its device KEY over the node's challenge; COMMON-W1R R6 and patch P-S9e-4: the frames are
//! azlin-creds', the reason words are azlin-regbase's).
//!
//! - [`RefreshRequest`]: the account, WHO refreshes (a device, or an app principal), the PROOF (the
//!   family token of a device, or a key proof over the node's challenge: `azlin_regbase::proof`), and the
//!   client's version (the `min_client` rule).
//! - [`RefreshAnswerWire`]: either a grant (the credential's bytes, the rotated token, the day the family
//!   runs out, notices) or a refusal (the machine word of the reason, whether the token was spent, the end
//!   of a hold, the minimum client version).
//!
//! BOTH CARRY A BEARER SECRET (the token's 32-byte secret), so they follow COMMON-W1R R17: they are
//! DECODED with `azlin_cbor::decode_secret` (the decoded tree wipes its buffers when it drops, and when
//! the decoder refuses its input half way), the secret is copied at once into a `Secret<32>`, and they are
//! ENCODED by a small canonical writer into a buffer that is sized up front and wiped on drop (a growing
//! buffer would leave unwiped copies behind, and the secret never passes through a `Value`). The writer
//! is checked against the strict decoder, and the bytes are pinned by the golden vectors.
//!
//! The frames are not signed and are not copied between versions (a refresh request is made fresh every
//! time): a key the reader does not know is ignored, `crit` is honoured, and nothing is re-written.

use core::fmt;

use azlin_cbor::{decode_secret, IntMap, Limits, Value};
use azlin_keys::Secret;
use azlin_regbase::{
    AccountId, Challenge, ClientVersion, DayStart, DeviceId, FamilyId, KeyProof, LeaderTime,
    MemberId, RefusalWord, Signature64, SmallText,
};
use zeroize::{Zeroize, Zeroizing};

use crate::canon::Short;
use crate::error::{CanonicalError, WireError};

/// The most notices an answer carries (planning/architecture.md section 1.4 rule 4). est.
pub const MAX_NOTICES: usize = 4;

/// The longest credential an answer carries: the limit of a signed structure.
pub const CREDENTIAL_MAX: usize = 8 * 1024;

/// The room a request's writer reserves: a request is about a hundred bytes, so the buffer never grows.
const REQUEST_CAP: usize = 256;

/// The limits of a request as it arrives.
const REQUEST_LIMITS: Limits = Limits {
    max_input: 512,
    max_depth: 3,
    max_items: 16,
    max_total_items: 64,
    max_text: 128,
};

/// The limits of an answer as it arrives: a credential and a few small fields.
const ANSWER_LIMITS: Limits = Limits {
    max_input: 12 * 1024,
    max_depth: 3,
    max_items: 16,
    max_total_items: 64,
    max_text: 9 * 1024,
};

// The wire keys of a request.
const R_ACCOUNT: u64 = 1;
const R_SUBJECT_KIND: u64 = 2;
const R_SUBJECT: u64 = 3;
const R_PROOF_KIND: u64 = 4;
const R_FAMILY: u64 = 5;
const R_SECRET: u64 = 6;
const R_CHALLENGE: u64 = 7;
const R_SIGNATURE: u64 = 8;
const R_CLIENT: u64 = 9;

// The numbers of the subject and proof kinds.
const SUBJECT_DEVICE: u64 = 1;
const SUBJECT_APP: u64 = 2;
const PROOF_TOKEN: u64 = 1;
const PROOF_KEY: u64 = 2;

// The wire keys of an answer.
const A_CREDENTIAL: u64 = 1;
const A_TOKEN_FAMILY: u64 = 2;
const A_TOKEN_SECRET: u64 = 3;
const A_FAMILY_EXPIRES: u64 = 4;
const A_NOTICES: u64 = 5;
const A_REASON: u64 = 6;
const A_SPENT_NOTHING: u64 = 7;
const A_HOLD_UNTIL: u64 = 8;
const A_MIN_CLIENT: u64 = 9;

/// Who refreshes: a device (it holds a rotating family) or an app principal (it holds a key pair and no
/// family: planning/architecture.md section 5.3).
///
/// Caller: `RefreshRequest`; S9b's `RefreshSubject` is built from it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RefreshSubject {
    /// A device of the account.
    Device(DeviceId),
    /// An app principal of the account (a member of kind `app`).
    App(MemberId),
}

impl fmt::Debug for RefreshSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefreshSubject::Device(d) => write!(f, "Device({:?})", Short(d.as_bytes())),
            RefreshSubject::App(m) => write!(f, "App({:?})", Short(m.as_bytes())),
        }
    }
}

/// The token of a device family: the family's id and the secret half (32 bytes). A BEARER SECRET: it has
/// no `Clone`, no `Copy`, no `Default`, no `PartialEq`, its `Debug` prints `***`, and the secret is wiped
/// when the value drops (`Secret<32>`).
///
/// Implements planning/architecture.md section 5.3 (`reg_family`: the token's hash is stored, the token
/// is what the device holds).
///
/// Caller: S9h (the client builds one from its stored token to refresh, and receives the rotated one in
/// the answer), S9b's `prepare_refresh` (the node hashes the secret with `TokenHash::of_secret`).
pub struct FamilyToken {
    family: FamilyId,
    secret: Secret<32>,
}

impl FamilyToken {
    /// The token of this family and secret.
    pub fn new(family: FamilyId, secret: Secret<32>) -> FamilyToken {
        FamilyToken { family, secret }
    }

    /// The family.
    pub fn family(&self) -> FamilyId {
        self.family
    }

    /// The secret half: hash it with `azlin_regbase::TokenHash::of_secret`, never print it.
    pub fn secret(&self) -> &Secret<32> {
        &self.secret
    }
}

impl fmt::Debug for FamilyToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FamilyToken({:?}, secret ***)", self.family)
    }
}

/// What a client offers to prove it may refresh: the family token of a device, or a signature with the
/// device key (or the app principal's key) over the node's challenge. The token is for a DEVICE only: an
/// app principal never rotates a token.
///
/// Caller: `RefreshRequest`; S9b's `DeviceProof` and `AppProof` are built from it.
pub enum RefreshProof {
    /// The device's family token.
    Token(FamilyToken),
    /// A key proof over the node's challenge (`azlin_regbase::proof`).
    Key(KeyProof),
}

impl fmt::Debug for RefreshProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefreshProof::Token(token) => write!(f, "Token({token:?})"),
            RefreshProof::Key(proof) => write!(
                f,
                "Key(challenge {:?}, signature {:?})",
                Short(proof.challenge.as_bytes()),
                Short(proof.signature.as_bytes())
            ),
        }
    }
}

/// A refresh request.
///
/// Implements planning/architecture.md section 2.4 ("refresh: device key or token") and DRIFT X-D-07.
///
/// Caller: S9h (the client builds it and sends `encode`'s bytes), the verb handler of S6 (`decode`), S9b
/// (`prepare_refresh` turns it into its command).
pub struct RefreshRequest {
    /// The account.
    pub account: AccountId,
    /// Who refreshes.
    pub subject: RefreshSubject,
    /// The proof.
    pub proof: RefreshProof,
    /// The client's version (the account's `min_client` rule).
    pub client: ClientVersion,
}

impl fmt::Debug for RefreshRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshRequest")
            .field("account", &Short(self.account.as_bytes()))
            .field("subject", &self.subject)
            .field("proof", &self.proof)
            .field("client", &self.client)
            .finish()
    }
}

/// A canonical CBOR writer into a buffer that was sized up front and is wiped when it drops: the bytes of
/// a token's secret never pass through a `Value` or a buffer that grows (COMMON-W1R R17).
struct Out(Zeroizing<Vec<u8>>);

impl Out {
    fn with_capacity(room: usize) -> Out {
        Out(Zeroizing::new(Vec::with_capacity(room)))
    }

    /// The head of an item: the major type and its argument, in the shortest form.
    fn head(&mut self, major: u8, argument: u64) {
        let lead = major << 5;
        match u8::try_from(argument) {
            Ok(small) if small < 24 => self.0.push(lead | small),
            Ok(byte) => {
                self.0.push(lead | 24);
                self.0.push(byte);
            }
            Err(_) => match u16::try_from(argument) {
                Ok(word) => {
                    self.0.push(lead | 25);
                    self.0.extend_from_slice(&word.to_be_bytes());
                }
                Err(_) => match u32::try_from(argument) {
                    Ok(long) => {
                        self.0.push(lead | 26);
                        self.0.extend_from_slice(&long.to_be_bytes());
                    }
                    Err(_) => {
                        self.0.push(lead | 27);
                        self.0.extend_from_slice(&argument.to_be_bytes());
                    }
                },
            },
        }
    }

    fn uint(&mut self, number: u64) {
        self.head(0, number);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        // A `usize` fits a `u64` on every supported target.
        self.head(2, bytes.len() as u64);
        self.0.extend_from_slice(bytes);
    }

    fn text(&mut self, text: &str) {
        self.head(3, text.len() as u64);
        self.0.extend_from_slice(text.as_bytes());
    }

    fn array(&mut self, items: u64) {
        self.head(4, items);
    }

    fn map(&mut self, entries: u64) {
        self.head(5, entries);
    }

    fn version(&mut self, v: ClientVersion) {
        self.array(3);
        self.uint(u64::from(v.major()));
        self.uint(u64::from(v.minor()));
        self.uint(u64::from(v.patch()));
    }

    fn finish(self) -> Zeroizing<Vec<u8>> {
        self.0
    }
}

/// A client version from a wire array of three numbers.
fn read_version(items: &[Value]) -> Result<ClientVersion, WireError> {
    match items {
        [Value::Uint(major), Value::Uint(minor), Value::Uint(patch)] => {
            let part = |n: &u64| u16::try_from(*n).map_err(|_| WireError::Malformed);
            Ok(ClientVersion::new(part(major)?, part(minor)?, part(patch)?))
        }
        _ => Err(WireError::Malformed),
    }
}

impl RefreshRequest {
    /// The canonical bytes of the request, in a buffer that is wiped when it drops. An app principal
    /// offering a TOKEN is refused (`NotCanonical`): app principals never rotate one.
    ///
    /// Caller: S9h (the client's request), the tests.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, CanonicalError> {
        if matches!(
            (&self.subject, &self.proof),
            (RefreshSubject::App(_), RefreshProof::Token(_))
        ) {
            return Err(CanonicalError::NotCanonical);
        }
        let mut out = Out::with_capacity(REQUEST_CAP);
        out.map(7);
        out.uint(R_ACCOUNT);
        out.bytes(self.account.as_bytes());
        out.uint(R_SUBJECT_KIND);
        match &self.subject {
            RefreshSubject::Device(device) => {
                out.uint(SUBJECT_DEVICE);
                out.uint(R_SUBJECT);
                out.bytes(device.as_bytes());
            }
            RefreshSubject::App(member) => {
                out.uint(SUBJECT_APP);
                out.uint(R_SUBJECT);
                out.bytes(member.as_bytes());
            }
        }
        out.uint(R_PROOF_KIND);
        match &self.proof {
            RefreshProof::Token(token) => {
                out.uint(PROOF_TOKEN);
                out.uint(R_FAMILY);
                out.bytes(token.family.as_bytes());
                out.uint(R_SECRET);
                out.bytes(token.secret.expose());
            }
            RefreshProof::Key(proof) => {
                out.uint(PROOF_KEY);
                out.uint(R_CHALLENGE);
                out.bytes(proof.challenge.as_bytes());
                out.uint(R_SIGNATURE);
                out.bytes(proof.signature.as_bytes());
            }
        }
        out.uint(R_CLIENT);
        out.version(self.client);
        Ok(out.finish())
    }

    /// The request of canonical bytes, STRICTLY (the codec's checks, exactly one proof whose fields are
    /// all there, a token only for a device); the decoded tree is wiped when it drops and the token's
    /// secret goes straight into a `Secret<32>`. A key nobody knows is ignored unless `crit` lists it.
    ///
    /// Caller: the verb handler of S6, the tests.
    pub fn decode(bytes: &[u8]) -> Result<RefreshRequest, WireError> {
        let tree = decode_secret(bytes, &REQUEST_LIMITS)?;
        let mut map = IntMap::new(tree.as_value())?;
        let account = AccountId::from_bytes(map.req_bytes_n::<16>(R_ACCOUNT)?);
        let subject_kind = map.req_u64(R_SUBJECT_KIND)?;
        let subject_id = map.req_bytes_n::<16>(R_SUBJECT)?;
        let subject = match subject_kind {
            SUBJECT_DEVICE => RefreshSubject::Device(DeviceId::from_bytes(subject_id)),
            SUBJECT_APP => RefreshSubject::App(MemberId::from_bytes(subject_id)),
            _ => return Err(WireError::Malformed),
        };
        let proof_kind = map.req_u64(R_PROOF_KIND)?;
        let family = map.opt_bytes_n::<16>(R_FAMILY)?;
        // The wrapper wipes whatever is left of the secret on every path out of this function.
        let mut token_secret = Zeroizing::new(map.opt_bytes_n::<32>(R_SECRET)?);
        let challenge = map.opt_bytes_n::<16>(R_CHALLENGE)?;
        let signature = map.opt_bytes_n::<64>(R_SIGNATURE)?;
        let client = read_version(map.req_array(R_CLIENT)?)?;
        map.finish()?;
        let proof = match (
            proof_kind,
            family,
            Option::as_mut(&mut token_secret),
            challenge,
            signature,
        ) {
            (PROOF_TOKEN, Some(family), Some(raw), None, None) => {
                let secret = Secret::from_bytes(*raw);
                raw.zeroize();
                Ok(RefreshProof::Token(FamilyToken::new(
                    FamilyId::from_bytes(family),
                    secret,
                )))
            }
            (PROOF_KEY, None, None, Some(challenge), Some(signature)) => {
                Ok(RefreshProof::Key(KeyProof {
                    challenge: Challenge::from_bytes(challenge),
                    signature: Signature64::from_bytes(signature),
                }))
            }
            _ => Err(WireError::Malformed),
        };
        let proof = proof?;
        if matches!(
            (&subject, &proof),
            (RefreshSubject::App(_), RefreshProof::Token(_))
        ) {
            return Err(WireError::Malformed);
        }
        Ok(RefreshRequest {
            account,
            subject,
            proof,
            client,
        })
    }
}

/// A refresh answer: a grant, or a refusal (exactly one of `credential` and `reason`).
///
/// - A GRANT has `credential` (the bytes of a `Credential`), a rotated `token` for a device (none for an
///   app principal), the day the family runs out (`family_expires_at`; none for an app principal) and
///   `notices`; it has no reason, no hold and no minimum version, and `spent_nothing` is false.
/// - A REFUSAL has the `reason` word, whether the token was spent (`spent_nothing`: the same token may be
///   sent again at once), and where the reason has them the end of a hold and the minimum client version.
///
/// `notices` are machine words (`SmallText`, at most [`MAX_NOTICES`]); their vocabulary is S9b's and
/// S9h's (DRIFT X-DELTA-W1S9E-11): a client ignores a word it does not know.
///
/// The constructors [`RefreshAnswerWire::granted`] and [`RefreshAnswerWire::refused`] make the two shapes;
/// encode and decode refuse every other combination.
///
/// Implements planning/architecture.md section 2.4 and DRIFT X-D-07, X-D-16 (a `reason` beside the
/// status).
///
/// Caller: S9i (`finish_refresh` builds it), the verb handler of S6 (`encode`), S9h (`decode`).
pub struct RefreshAnswerWire {
    /// The bytes of the credential; present exactly when the refresh was granted.
    pub credential: Option<Vec<u8>>,
    /// The rotated token of a device; present only with a credential.
    pub token: Option<FamilyToken>,
    /// The day the family runs out (a day start); present only with a credential.
    pub family_expires_at: Option<DayStart>,
    /// Machine words for the client; present only with a credential.
    pub notices: Vec<SmallText>,
    /// Why the refresh was refused; present exactly when the refresh was refused.
    pub reason: Option<RefusalWord>,
    /// A refusal that spent nothing: the same token may ask again at once.
    pub spent_nothing: bool,
    /// The end of the enrollment hold of a `device_pending` refusal.
    pub hold_until: Option<LeaderTime>,
    /// The minimum client version of a `client_too_old` refusal.
    pub min_client: Option<ClientVersion>,
}

impl fmt::Debug for RefreshAnswerWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshAnswerWire")
            .field("credential_len", &self.credential.as_ref().map(Vec::len))
            .field("token", &self.token)
            .field("family_expires_at", &self.family_expires_at)
            .field("notices", &self.notices)
            .field("reason", &self.reason)
            .field("spent_nothing", &self.spent_nothing)
            .field("hold_until", &self.hold_until)
            .field("min_client", &self.min_client)
            .finish()
    }
}

impl RefreshAnswerWire {
    /// A grant: the credential's bytes, the rotated token of a device, the day the family runs out, the
    /// notices.
    pub fn granted(
        credential: Vec<u8>,
        token: Option<FamilyToken>,
        family_expires_at: Option<DayStart>,
        notices: Vec<SmallText>,
    ) -> RefreshAnswerWire {
        RefreshAnswerWire {
            credential: Some(credential),
            token,
            family_expires_at,
            notices,
            reason: None,
            spent_nothing: false,
            hold_until: None,
            min_client: None,
        }
    }

    /// A refusal: the reason word, whether the token was spent, the end of a hold, the minimum version.
    pub fn refused(
        reason: RefusalWord,
        spent_nothing: bool,
        hold_until: Option<LeaderTime>,
        min_client: Option<ClientVersion>,
    ) -> RefreshAnswerWire {
        RefreshAnswerWire {
            credential: None,
            token: None,
            family_expires_at: None,
            notices: Vec::new(),
            reason: Some(reason),
            spent_nothing,
            hold_until,
            min_client,
        }
    }

    /// Whether the fields make one of the two shapes (see the type).
    fn is_well_formed(&self) -> bool {
        if self.notices.len() > MAX_NOTICES || self.notices.iter().any(|n| !n.is_word()) {
            return false;
        }
        match (&self.credential, self.reason) {
            (Some(credential), None) => {
                !credential.is_empty()
                    && credential.len() <= CREDENTIAL_MAX
                    && !self.spent_nothing
                    && self.hold_until.is_none()
                    && self.min_client.is_none()
            }
            (None, Some(_)) => {
                self.token.is_none() && self.family_expires_at.is_none() && self.notices.is_empty()
            }
            _ => false,
        }
    }

    /// The canonical bytes of the answer, in a buffer that is wiped when it drops. Refused: any
    /// combination of fields that is neither a grant nor a refusal (`NotCanonical`).
    ///
    /// Caller: the verb handler of S6 (the answer's body), the tests.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, CanonicalError> {
        if !self.is_well_formed() {
            return Err(CanonicalError::NotCanonical);
        }
        let credential_len = self.credential.as_ref().map_or(1, Vec::len);
        let mut out = Out::with_capacity(credential_len + 512);
        let mut entries: u64 = 0;
        entries += u64::from(self.credential.is_some());
        entries += if self.token.is_some() { 2 } else { 0 };
        entries += u64::from(self.family_expires_at.is_some());
        entries += u64::from(!self.notices.is_empty());
        entries += u64::from(self.reason.is_some());
        entries += u64::from(self.spent_nothing);
        entries += u64::from(self.hold_until.is_some());
        entries += u64::from(self.min_client.is_some());
        out.map(entries);
        if let Some(credential) = &self.credential {
            out.uint(A_CREDENTIAL);
            out.bytes(credential);
        }
        if let Some(token) = &self.token {
            out.uint(A_TOKEN_FAMILY);
            out.bytes(token.family.as_bytes());
            out.uint(A_TOKEN_SECRET);
            out.bytes(token.secret.expose());
        }
        if let Some(day) = self.family_expires_at {
            out.uint(A_FAMILY_EXPIRES);
            out.uint(day.start().as_secs());
        }
        if !self.notices.is_empty() {
            out.uint(A_NOTICES);
            out.array(self.notices.len() as u64);
            for notice in &self.notices {
                out.text(notice.as_str());
            }
        }
        if let Some(reason) = self.reason {
            out.uint(A_REASON);
            out.text(reason.as_str());
        }
        if self.spent_nothing {
            out.uint(A_SPENT_NOTHING);
            out.0.push(0xf5);
        }
        if let Some(hold) = self.hold_until {
            out.uint(A_HOLD_UNTIL);
            out.uint(hold.as_secs());
        }
        if let Some(min) = self.min_client {
            out.uint(A_MIN_CLIENT);
            out.version(min);
        }
        Ok(out.finish())
    }

    /// The answer of canonical bytes, STRICTLY (the codec's checks, exactly one of a credential and a
    /// reason, the words of their vocabularies, `spent_nothing` present only when true, at most
    /// [`MAX_NOTICES`] notices); the decoded tree is wiped when it drops and the token's secret goes
    /// straight into a `Secret<32>`. A key nobody knows is ignored unless `crit` lists it.
    ///
    /// Caller: S9h (the client reads the node's answer), the tests.
    pub fn decode(bytes: &[u8]) -> Result<RefreshAnswerWire, WireError> {
        let tree = decode_secret(bytes, &ANSWER_LIMITS)?;
        let mut map = IntMap::new(tree.as_value())?;
        let credential = map.opt_bytes(A_CREDENTIAL)?.map(<[u8]>::to_vec);
        let family = map.opt_bytes_n::<16>(A_TOKEN_FAMILY)?;
        // The wrapper wipes whatever is left of the secret on every path out of this function.
        let mut token_secret = Zeroizing::new(map.opt_bytes_n::<32>(A_TOKEN_SECRET)?);
        let family_expires_at = match map.opt_u64(A_FAMILY_EXPIRES)? {
            Some(number) => Some(
                DayStart::from_start(LeaderTime::from_secs(number))
                    .ok_or(WireError::OutOfRange(A_FAMILY_EXPIRES))?,
            ),
            None => None,
        };
        let mut notices: Vec<SmallText> = Vec::new();
        if let Some(items) = map.opt_array(A_NOTICES)? {
            if items.is_empty() {
                // The writer leaves the key out: an empty array would be a second encoding.
                return Err(WireError::NotCanonical);
            }
            if items.len() > MAX_NOTICES {
                return Err(WireError::TooMany);
            }
            for item in items {
                let Value::Text(word) = item else {
                    return Err(WireError::Malformed);
                };
                notices.push(SmallText::new(word).map_err(|_| WireError::Malformed)?);
            }
        }
        let reason = match map.opt_text(A_REASON)? {
            Some(word) => Some(
                word.parse::<RefusalWord>()
                    .map_err(|_| WireError::UnknownWord)?,
            ),
            None => None,
        };
        let spent_nothing = match map.opt_bool(A_SPENT_NOTHING)? {
            Some(true) => true,
            Some(false) => return Err(WireError::NotCanonical),
            None => false,
        };
        let hold_until = map.opt_u64(A_HOLD_UNTIL)?.map(LeaderTime::from_secs);
        let min_client = match map.opt_array(A_MIN_CLIENT)? {
            Some(items) => Some(read_version(items)?),
            None => None,
        };
        map.finish()?;
        let token = match (family, Option::as_mut(&mut token_secret)) {
            (Some(family), Some(raw)) => {
                let secret = Secret::from_bytes(*raw);
                raw.zeroize();
                Ok(Some(FamilyToken::new(FamilyId::from_bytes(family), secret)))
            }
            (None, None) => Ok(None),
            _ => Err(WireError::Malformed),
        };
        let answer = RefreshAnswerWire {
            credential,
            token: token?,
            family_expires_at,
            notices,
            reason,
            spent_nothing,
            hold_until,
            min_client,
        };
        if !answer.is_well_formed() {
            return Err(WireError::Malformed);
        }
        Ok(answer)
    }
}
