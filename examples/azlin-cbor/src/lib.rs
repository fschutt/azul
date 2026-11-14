//! Strict deterministic CBOR: the one codec of the Azlin engine's wire and signed formats
//! (planning/architecture.md section 5.1 principle 6 and section 2.5; build unit S14, lane
//! W1-S14a). Every format of section 5 (pointer v2, pack trailer, chunk index shards, envelope v2,
//! team log, space chains, credentials, request headers) is written and read with this crate, in
//! azul and in azul-apps.
//!
//! The profile is the core deterministic encoding of RFC 8949 section 4.2.1 on what the formats
//! need: unsigned and negative integers, byte strings, text strings, arrays, maps, `true`,
//! `false` and `null`. Floats, tags, `undefined`, other simple values, indefinite lengths and the
//! break are refused with a typed error, as are non-shortest integers and lengths, unsorted and
//! duplicate map keys, invalid UTF-8 and trailing bytes. The one rule that follows, THE SIGNATURE
//! RULE: for every `b` that [`decode`] accepts, `encode(decode(b)) == b`; a signature covers
//! canonical bytes, and whoever copies a structure keeps its unknown fields (see [`IntMap`]).
//!
//! No dependencies (std only), no unsafe code, no clock, no randomness, no I/O: a pure codec
//! (planning/architecture.md section 1.4 rules 3 and 14). It builds for `wasm32-unknown-unknown`.
//!
//! ```
//! use azlin_cbor::{decode, encode, IntMap, IntMapBuilder, Limits, ShapeError, Value};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A struct: key 1 = version, key 2 = a 4-byte id, key 7 = a field a newer writer added.
//! let wire = encode(
//!     &IntMapBuilder::new()
//!         .put(1, Value::Uint(2))
//!         .put(2, Value::Bytes(vec![1, 2, 3, 4]))
//!         .put(7, Value::Text("newer".into()))
//!         .build()?,
//!     &Limits::OBJECT,
//! )?;
//!
//! let value = decode(&wire, &Limits::OBJECT)?;
//! let mut m = IntMap::new(&value)?;
//! assert_eq!(m.req_u64(1)?, 2);
//! assert_eq!(m.req_bytes_n::<4>(2)?, [1, 2, 3, 4]);
//! let unknown = m.finish()?; // key 7 is ignored, and returned so a copy can keep it
//! assert_eq!(unknown.len(), 1);
//!
//! // A key the sender lists in `crit` that this reader never asked for refuses the message.
//! let strict = IntMapBuilder::new().put(1, Value::Uint(2)).put(9, Value::Null).crit(&[9]).build()?;
//! let mut m = IntMap::new(&strict)?;
//! assert_eq!(m.req_u64(1)?, 2);
//! assert_eq!(m.finish().err(), Some(ShapeError::CritUnknown(9)));
//! # Ok(())
//! # }
//! ```

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
#![warn(missing_docs)]

pub mod dec;
pub mod enc;
pub mod error;
pub mod int_map;
pub mod limits;
pub mod value;

pub use dec::decode;
pub use enc::encode;
pub use error::{DecodeError, EncodeError, MapError, ShapeError};
pub use int_map::{IntMap, IntMapBuilder};
pub use limits::Limits;
pub use value::{Map, Value};
