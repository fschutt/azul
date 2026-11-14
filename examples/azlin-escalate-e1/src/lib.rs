//! owned by lane W1-S47a
//!
//! Azlin escalate E1: the independent reference model of the team log's permission check and its
//! differential test (planning/architecture.md section 12 (E1), section 5.9 the table of ops and who may sign,
//! section 7.2 roles and scopes, section 7.3 the identity service's delegation, section 7.5 the quorum paths,
//! section 5.10 the policy; build unit S47, lane W1-S47a).
//!
//! E1's claim: "no entry is accepted unless its signer may sign it; a quorum counts distinct people, never one
//! person's devices". This crate checks it the way the plan says: a SEPARATE, deliberately naive
//! implementation of the permission check (the reference model, written from the plan's text and the
//! authority table v0, which is the owner's to ratify: ARCH29 9.13 X-D-54), a seeded generator of sequences
//! of ops, group moves and delegations (valid, and wrong in exactly one place on purpose), and a differential
//! test that runs three things on every entry and demands that they agree: the model, the production check as
//! the metadata group runs it (`azlin_teamlog::verify_signatures` + `admit` + `apply`) and a device's own
//! replay (`azlin_teamlog::replay`). Hand-written stories attack the table itself: "can a member of rank X
//! make themselves rank Y".
//!
//! | module | what | the plan |
//! |---|---|---|
//! | [`model`] | the reference model: ids, roles, entries, state, `model_admit`, `model_apply`; imports nothing from `azlin-teamlog` | 5.9, 7.2, 7.3, 7.5, 5.10 |
//! | [`universe`] | the universe of an account (members, roles, groups, devices, delegations) and the bytes of every id and key | 7.2 |
//! | [`gen`] | the generator: the setup of a universe, valid entries, near misses, random entries | 12 E1 |
//! | [`project`] | both sides' states as one plain `StateView` | 12 E1 |
//! | [`compare`] | the converter, the reviewed class table, the rig, `differential`, `run_steps`, `shrink` | 12 E1 |
//! | [`stories`] | the story rig and the team world the attack stories start from | 12 E1 |
//!
//! A finding (a production bug, or a hole of the table) is never a test to loosen: it is a row of
//! `notes/audits/closeout-2026-11/delta/W1S47A.tsv` and, where it is the production's, a test marked
//! `#[ignore = "X-DELTA-W1S47A-<n>"]` that shows it.
//!
//! This crate is a test-support library (`publish = false`): it does no I/O, reads no clock and draws no
//! entropy (the seeded generator is `azlin_seams::sim::Rng`, the signatures are `azlin_seams::FakeScheme`).

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
#![warn(missing_docs)]

pub mod model;
pub mod universe;

pub mod gen;
pub mod project;

pub mod compare;
pub mod stories;
