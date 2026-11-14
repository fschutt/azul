//! The hashes of the team log: the canonical `state_hash` of a `TeamState` and the hash of an entry
//! (planning/architecture.md section 5.9 "Checkpoints: every 1,000 entries, and monthly, the group
//! records a checkpoint (the log's head and a hash of the registry state it produced)"; build unit S17,
//! lane W1-S17c1).
//!
//! [`TeamState::state_hash`] is BLAKE3 (`azlin_seams::hash32` under [`STATE_HASH_DOMAIN`]) of a
//! canonical encoding of EVERY field of the state, written below in the order of the fields. It is
//! independent of the order members, devices and groups were inserted in, because every collection of
//! the state is ordered and the encoding walks it in that order. The encoding is self-delimiting (a
//! count before every collection, a length before every byte string, a tag before every optional or
//! variant), so two different states never encode to the same bytes. A field that is added to a
//! structure of the state and is not written here does not compile: every encoder destructures its
//! structure with ALL its fields named.
//!
//! The device that verifies a chain from a checkpoint (planning/architecture.md section 5.9) fetches
//! the state the checkpoint's `state_hash` names and compares: the checkpoint is the group's claim,
//! this hash is the device's own replay.

use azlin_policy::{
    LinkPolicy, LinkPolicyPatch, Policy, PolicyPatch, RequestPolicy, RequestPolicyPatch,
};
use azlin_seams::{hash32, Hash32};

use crate::devices::{Device, DeviceState, PublicKeys};
use crate::groups::Group;
use crate::members::{AppSpec, Invite, Member};
use crate::state::{Delegation, EntitlementFacts, PendingOpen, TeamState};

/// The domain of the state hash: a `derive_key` context no other hash of the engine uses.
///
/// Implements planning/architecture.md section 2.5 (each structure has its own domain string) and
/// section 5.9 (a hash of the registry state). The string is a hash domain, not a signature domain:
/// drift row of lane W1-S17c.
///
/// Caller: `TeamState::state_hash`.
pub const STATE_HASH_DOMAIN: &str = "azlin-teamlog-state-v1";

/// The domain of the hash of an entry: the hash a chain's `prev` names and `hash` carries.
///
/// Implements planning/architecture.md section 5.9 (hash-chained: each entry names its predecessor)
/// and section 2.5 (each structure has its own domain string). A hash domain, not a signature domain:
/// drift row of lane W1-S17c; the golden vectors of the glue (S5) fix the bytes it hashes.
///
/// Caller: `entry_hash`.
pub const ENTRY_HASH_DOMAIN: &str = "azlin-teamlog-entry-v1";

/// The hash of an entry: BLAKE3 of its `preimage` under [`ENTRY_HASH_DOMAIN`]. The preimage is the
/// byte string every signature of the entry covers (`domain || 0x00 || canonical bytes`, built by the
/// glue), so a signature over the preimage is a signature over this hash.
///
/// Implements planning/architecture.md section 5.9 ("every entry is signed by a device"; "an entry
/// that fails its hash or signature check is fetched again from another voter").
///
/// Caller: `verify::verify_signatures` (the copy is intact when its hash is the hash of its
/// preimage), the glue and the tests that build entries.
pub fn entry_hash(preimage: &[u8]) -> Hash32 {
    hash32(ENTRY_HASH_DOMAIN, &[preimage])
}

/// A writer of the canonical encoding.
struct Enc(Vec<u8>);

impl Enc {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    /// A count: the number of items of a collection that follows.
    fn count(&mut self, value: usize) {
        // A `usize` fits a `u64` on every target this engine builds for.
        self.u64(value as u64);
    }

    /// Bytes of a fixed size that the type fixes (an id, a key, a hash).
    fn raw(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    /// Bytes of any size: the length first.
    fn bytes(&mut self, value: &[u8]) {
        self.count(value.len());
        self.raw(value);
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    /// An optional value: a tag, then the value when there is one.
    fn opt<T>(&mut self, value: Option<T>, write: impl FnOnce(&mut Enc, T)) {
        match value {
            None => self.u8(0),
            Some(inner) => {
                self.u8(1);
                write(self, inner);
            }
        }
    }
}

fn span_opt(enc: &mut Enc, value: Option<azlin_seams::Span>) {
    enc.opt(value, |enc, span| enc.u64(span.as_secs()));
}

fn keys(enc: &mut Enc, value: &PublicKeys) {
    let PublicKeys { sign, kx, pq } = value;
    enc.raw(sign);
    enc.raw(kx);
    enc.opt(pq.as_deref(), |enc, pq| enc.bytes(pq));
}

fn member(enc: &mut Enc, value: &Member) {
    let Member {
        id,
        kind,
        role,
        admin_scope,
        managed,
        state,
        keys: member_keys,
        invite,
        app,
        added_at,
        removed_at,
        log_seq,
    } = value;
    enc.raw(id.as_bytes());
    enc.str(kind.as_str());
    enc.str(role.as_str());
    enc.opt(admin_scope.as_ref(), |enc, group| enc.raw(group.as_bytes()));
    enc.bool(*managed);
    enc.str(state.word());
    enc.opt(member_keys.as_ref(), keys);
    enc.opt(invite.as_ref(), |enc, invite| {
        let Invite { hash, inviter } = invite;
        enc.raw(hash.as_bytes());
        enc.opt(inviter.as_ref(), |enc, member| enc.raw(member.as_bytes()));
    });
    enc.opt(app.as_ref(), |enc, app| {
        let AppSpec { scopes, expires_at } = app;
        enc.count(scopes.len());
        for scope in scopes {
            enc.str(scope.as_str());
        }
        enc.opt(*expires_at, |enc, at| enc.u64(at.as_secs()));
    });
    enc.u64(added_at.as_secs());
    enc.opt(*removed_at, |enc, at| enc.u64(at.as_secs()));
    enc.u64(log_seq.get());
}

fn device(enc: &mut Enc, value: &Device) {
    let Device {
        id,
        member,
        keys: device_keys,
        state,
        enrolled_by,
        label_sealed,
        added_at,
        revoked_at,
        log_seq,
    } = value;
    enc.raw(id.as_bytes());
    enc.raw(member.as_bytes());
    keys(enc, device_keys);
    match state {
        DeviceState::Pending { hold_until } => {
            enc.u8(0);
            enc.u64(hold_until.as_secs());
        }
        DeviceState::Active => enc.u8(1),
        DeviceState::Revoked => enc.u8(2),
    }
    enc.str(enrolled_by.word());
    enc.opt(label_sealed.as_deref(), |enc, sealed| enc.bytes(sealed));
    enc.u64(added_at.as_secs());
    enc.opt(*revoked_at, |enc, at| enc.u64(at.as_secs()));
    enc.u64(log_seq.get());
}

fn group(enc: &mut Enc, value: &Group) {
    let Group {
        id,
        parent,
        name_sealed,
        log_seq,
    } = value;
    enc.raw(id.as_bytes());
    enc.opt(parent.as_ref(), |enc, parent| enc.raw(parent.as_bytes()));
    enc.opt(name_sealed.as_deref(), |enc, sealed| enc.bytes(sealed));
    enc.u64(log_seq.get());
}

fn link_policy(enc: &mut Enc, value: &LinkPolicy) {
    let LinkPolicy {
        audiences,
        max_expiry,
        password_for_anyone,
        force_preview_only,
        force_watermark,
        force_app_only,
        uploads,
        edit,
        comments,
        open_logs,
        slugs,
    } = value;
    let classes = audiences.classes();
    enc.count(classes.len());
    for class in classes {
        enc.str(class.as_str());
    }
    span_opt(enc, *max_expiry);
    for flag in [
        password_for_anyone,
        force_preview_only,
        force_watermark,
        force_app_only,
        uploads,
        edit,
        comments,
        open_logs,
        slugs,
    ] {
        enc.bool(*flag);
    }
}

fn request_policy(enc: &mut Enc, value: &RequestPolicy) {
    let RequestPolicy {
        allowed,
        max_expiry,
        max_files,
        max_bytes,
    } = value;
    enc.bool(*allowed);
    span_opt(enc, *max_expiry);
    enc.u32(*max_files);
    enc.u64(max_bytes.get());
}

fn policy(enc: &mut Enc, value: &Policy) {
    let Policy {
        credential_ttl,
        device_approval,
        max_devices_per_member,
        managed_default,
        sso_required,
        sso_max_age,
        enroll_hold,
        passkey_recovery,
        escrow_open_wait,
        audit_level,
        admin_quorum,
        link_policy: links,
        request_policy: requests,
        browser_devices,
        external_sharing,
        updates,
        padding,
    } = value;
    enc.u64(credential_ttl.as_secs());
    enc.str(device_approval.as_str());
    enc.u8(*max_devices_per_member);
    enc.str(managed_default.as_str());
    enc.bool(*sso_required);
    span_opt(enc, *sso_max_age);
    enc.u64(enroll_hold.as_secs());
    enc.bool(*passkey_recovery);
    enc.u64(escrow_open_wait.as_secs());
    enc.str(audit_level.as_str());
    enc.u8(admin_quorum.get());
    link_policy(enc, links);
    request_policy(enc, requests);
    enc.bool(*browser_devices);
    enc.str(external_sharing.as_str());
    enc.str(updates.as_str());
    enc.str(padding.as_str());
}

fn link_policy_patch(enc: &mut Enc, value: &LinkPolicyPatch) {
    let LinkPolicyPatch {
        audiences,
        max_expiry,
        password_for_anyone,
        force_preview_only,
        force_watermark,
        force_app_only,
        uploads,
        edit,
        comments,
        open_logs,
        slugs,
    } = value;
    enc.opt(audiences.as_ref(), |enc, set| {
        let classes = set.classes();
        enc.count(classes.len());
        for class in classes {
            enc.str(class.as_str());
        }
    });
    enc.opt(*max_expiry, span_opt);
    for flag in [
        password_for_anyone,
        force_preview_only,
        force_watermark,
        force_app_only,
        uploads,
        edit,
        comments,
        open_logs,
        slugs,
    ] {
        enc.opt(*flag, |enc, flag| enc.bool(flag));
    }
}

fn request_policy_patch(enc: &mut Enc, value: &RequestPolicyPatch) {
    let RequestPolicyPatch {
        allowed,
        max_expiry,
        max_files,
        max_bytes,
    } = value;
    enc.opt(*allowed, |enc, flag| enc.bool(flag));
    enc.opt(*max_expiry, span_opt);
    enc.opt(*max_files, |enc, files| enc.u32(files));
    enc.opt(*max_bytes, |enc, bytes| enc.u64(bytes.get()));
}

fn policy_patch(enc: &mut Enc, value: &PolicyPatch) {
    let PolicyPatch {
        credential_ttl,
        device_approval,
        max_devices_per_member,
        managed_default,
        sso_required,
        sso_max_age,
        enroll_hold,
        passkey_recovery,
        escrow_open_wait,
        audit_level,
        admin_quorum,
        link_policy: links,
        request_policy: requests,
        browser_devices,
        external_sharing,
        updates,
        padding,
    } = value;
    enc.opt(*credential_ttl, |enc, span| enc.u64(span.as_secs()));
    enc.opt(*device_approval, |enc, word| enc.str(word.as_str()));
    enc.opt(*max_devices_per_member, |enc, max| enc.u8(max));
    enc.opt(*managed_default, |enc, word| enc.str(word.as_str()));
    enc.opt(*sso_required, |enc, flag| enc.bool(flag));
    enc.opt(*sso_max_age, span_opt);
    enc.opt(*enroll_hold, |enc, span| enc.u64(span.as_secs()));
    enc.opt(*passkey_recovery, |enc, flag| enc.bool(flag));
    enc.opt(*escrow_open_wait, |enc, span| enc.u64(span.as_secs()));
    enc.opt(*audit_level, |enc, word| enc.str(word.as_str()));
    enc.opt(*admin_quorum, |enc, quorum| enc.u8(quorum.get()));
    link_policy_patch(enc, links);
    request_policy_patch(enc, requests);
    enc.opt(*browser_devices, |enc, flag| enc.bool(flag));
    enc.opt(*external_sharing, |enc, word| enc.str(word.as_str()));
    enc.opt(*updates, |enc, word| enc.str(word.as_str()));
    enc.opt(*padding, |enc, word| enc.str(word.as_str()));
}

fn delegation(enc: &mut Enc, value: &Delegation) {
    let Delegation {
        key,
        ops,
        expires,
        granted_by,
        granted_at,
        log_seq,
    } = value;
    enc.raw(key.as_bytes());
    enc.count(ops.len());
    for op in ops {
        enc.str(op.as_str());
    }
    enc.opt(*expires, |enc, at| enc.u64(at.as_secs()));
    enc.raw(granted_by.as_bytes());
    enc.u64(granted_at.as_secs());
    enc.u64(log_seq.get());
}

fn entitlement(enc: &mut Enc, value: &EntitlementFacts) {
    let EntitlementFacts {
        version,
        state,
        quota,
        seats,
        applied_at,
    } = value;
    enc.u64(version.0);
    enc.str(state.word());
    enc.u64(quota.get());
    enc.u32(*seats);
    enc.u64(applied_at.as_secs());
}

fn pending_open(enc: &mut Enc, value: &PendingOpen) {
    let PendingOpen {
        seq,
        member,
        reason,
        requested_at,
        ready_at,
        signers,
    } = value;
    enc.u64(seq.get());
    enc.raw(member.as_bytes());
    enc.str(reason.word());
    enc.u64(requested_at.as_secs());
    enc.opt(*ready_at, |enc, at| enc.u64(at.as_secs()));
    enc.count(signers.len());
    for signer in signers {
        enc.raw(signer.as_bytes());
    }
}

impl TeamState {
    /// The canonical encoding of the whole state: every field, every collection in order. The
    /// `state_hash` is the hash of these bytes.
    ///
    /// Implements planning/architecture.md section 5.9 (a hash of the registry state).
    ///
    /// Caller: `TeamState::state_hash`, the tests.
    pub(crate) fn canonical_bytes(&self) -> Vec<u8> {
        let TeamState {
            account,
            kind,
            genesis_hash,
            tip,
            members,
            devices,
            groups,
            policy: account_policy,
            group_patches,
            policy_epoch,
            delegations,
            escrow_pub,
            escrow_holders,
            audit_pub,
            seal_epoch,
            entitlement: latest_entitlement,
            pending_opens,
        } = self;
        let mut enc = Enc(Vec::new());
        enc.raw(account.as_bytes());
        enc.str(kind.as_str());
        enc.opt(genesis_hash.as_ref(), |enc, hash| enc.raw(hash.as_bytes()));
        // The chain: the head (the chain id is the account's), the leader's stamp of the newest entry
        // and the restore epoch.
        enc.opt(tip.head(), |enc, head| {
            enc.u64(head.seq.get());
            enc.raw(head.hash.as_bytes());
        });
        enc.opt(tip.last_at(), |enc, at| enc.u64(at.as_secs()));
        enc.u64(tip.epoch().get());
        enc.count(members.len());
        for value in members.values() {
            member(&mut enc, value);
        }
        enc.count(devices.len());
        for value in devices.values() {
            device(&mut enc, value);
        }
        enc.count(groups.len());
        for value in groups.iter() {
            group(&mut enc, value);
        }
        let pairs = groups.membership_pairs();
        enc.count(pairs.len());
        for (group_id, member_id) in &pairs {
            enc.raw(group_id.as_bytes());
            enc.raw(member_id.as_bytes());
        }
        policy(&mut enc, account_policy);
        enc.count(group_patches.len());
        for (group_id, patch) in group_patches {
            enc.raw(group_id.as_bytes());
            policy_patch(&mut enc, patch);
        }
        enc.u32(policy_epoch.get());
        enc.count(delegations.len());
        for value in delegations.values() {
            delegation(&mut enc, value);
        }
        enc.opt(escrow_pub.as_ref(), |enc, key| enc.raw(key));
        enc.count(escrow_holders.len());
        for holder in escrow_holders {
            enc.raw(holder.as_bytes());
        }
        enc.opt(audit_pub.as_ref(), |enc, key| enc.raw(key));
        enc.u32(seal_epoch.get());
        enc.opt(latest_entitlement.as_ref(), entitlement);
        enc.count(pending_opens.len());
        for value in pending_opens.values() {
            pending_open(&mut enc, value);
        }
        enc.0
    }

    /// The hash of the state: BLAKE3 under [`STATE_HASH_DOMAIN`] of the canonical encoding. Equal for
    /// two states that hold the same facts, whatever order the facts were inserted in, and different
    /// for two states that differ in any field.
    ///
    /// Implements planning/architecture.md section 5.9 ("the group records a checkpoint (the log's head
    /// and a hash of the registry state it produced)").
    ///
    /// Caller: the registry glue (the checkpoint writer), `replay` tests
    /// (`replay_from_checkpoint_state_equals_from_genesis`), lane S47a (the final state hash).
    pub fn state_hash(&self) -> Hash32 {
        let bytes = self.canonical_bytes();
        hash32(STATE_HASH_DOMAIN, &[&bytes])
    }
}
