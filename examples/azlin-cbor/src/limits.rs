//! The limits `decode` and `encode` work under: stated up front, read once, refused with a typed
//! error when reached (planning/architecture.md section 1.4 rule 4: "Everything has a limit").
//!
//! The three constants are the provisional numbers of P-3 of the brief (DRIFT X-DELTA-W1S14A-3
//! and X-DELTA-W1S14A-4): the plan gives only the sizes of a request header (16 KiB) and of a
//! `log_append` record (64 KiB), section 2.5. Section 2.5's limits table is S5's
//! (`azlin_contract::native`): when it names other numbers they move HERE, in this one place.

/// What one call of `decode` or `encode` accepts. Every field is a refusal point, never a
/// capacity: the decoder reserves memory only for what the input that is left can hold.
///
/// A scalar is at depth 0, `[1]` at depth 1, `[[1]]` at depth 2. Every data item counts once
/// against `max_total_items`: a map key, a map value, an array element, a container itself.
///
/// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...) passes one of the
/// three constants; INT-WIRE (azlin-proto contract4) passes `HEADER` for request headers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Limits {
    /// The most bytes `decode` accepts, and the most `encode` writes.
    pub max_input: u32,
    /// The deepest nesting of arrays and maps.
    pub max_depth: u8,
    /// The most items of one array, or pairs of one map.
    pub max_items: u32,
    /// The most data items in the whole value.
    pub max_total_items: u32,
    /// The most bytes of one byte string or text string.
    pub max_text: u32,
}

const KIB: u32 = 1024;
const MIB: u32 = 1024 * KIB;

impl Limits {
    /// A request header: 16 KiB, depth 8, 1024 items per container (planning/architecture.md
    /// section 2.5, the limits table: "request header 16 KiB"). DRIFT X-DELTA-W1S14A-3: depth,
    /// items and the total are provisional (the total is one item per four bytes of input).
    ///
    /// Caller: INT-WIRE (the `azlin/1` request header).
    pub const HEADER: Limits = Limits {
        max_input: 16 * KIB,
        max_depth: 8,
        max_items: 1024,
        max_total_items: 4 * KIB,
        max_text: 16 * KIB,
    };

    /// A `log_append` record: 64 KiB, depth 8, 1024 items per container (section 2.5: "log_append
    /// record 64 KiB"). DRIFT X-DELTA-W1S14A-3: as `HEADER`, the total is 16384.
    ///
    /// Caller: INT-WIRE (`log_append`, `team_log_append`), the team log and space chains.
    pub const LOG_RECORD: Limits = Limits {
        max_input: 64 * KIB,
        max_depth: 8,
        max_items: 1024,
        max_total_items: 16 * KIB,
        max_text: 64 * KIB,
    };

    /// Objects at rest: pointers, chunk lists, index shards, envelopes. 4 MiB, depth 12, 65536
    /// items per container, 1,048,576 items in all (P-3 of the brief; DRIFT X-DELTA-W1S14A-3).
    ///
    /// Caller: the format crates of sections 5.5 to 5.7 (azlin-keys envelope, azlin-content
    /// pointer, pack trailer and chunk index), INT-WIRE.
    pub const OBJECT: Limits = Limits {
        max_input: 4 * MIB,
        max_depth: 12,
        max_items: 65_536,
        max_total_items: MIB,
        max_text: 4 * MIB,
    };

    /// What no key of a map can exceed: the encoding of a key is made under these limits so that
    /// `Map::from_entries` can order keys it has no `Limits` for. Nothing larger can be encoded
    /// by any `Limits` value.
    pub(crate) const KEY: Limits = Limits {
        max_input: u32::MAX,
        max_depth: u8::MAX,
        max_items: u32::MAX,
        max_total_items: u32::MAX,
        max_text: u32::MAX,
    };
}
