//! Goldens made OUTSIDE the crate: SHA-256 by Python's hashlib, Ed25519 by OpenSSL 3.5, the Merkle tree by a
//! transcription of RFC 9162 (scratch/W1-S45c/gold/make_gold2.py, emit_rust_gold.py). Generated: do not edit.

/// The three releases of the golden log, as the canonical record texts.
pub const RECORDS: [&str; 3] = [
    "azlin-release-v1\nkind=desktop-app\nname=azdrive\nversion=0.3.0\nchannel=release\narch=any\ndigest=sha256:c063d290dc26b9d3f7919af90496f22473b15b0b0f0598c049662dff07e8cf64\nsigner=425ed4e4a36b30ea\nsecurity_fix=0\n",
    "azlin-release-v1\nkind=worker\nname=azmail-worker\nversion=2.0.1\nchannel=release\narch=any\ndigest=sha256:ca47608a8d5156885078f6cb0fcaf1ebb02556e67b7455162ead27e41cf2dac1\nsigner=425ed4e4a36b30ea\nsecurity_fix=1\n",
    "azlin-release-v1\nkind=browser-bundle\nname=drive-bundle\nversion=7\nchannel=release\narch=any\ndigest=sha256:30c396886a5639431d92117fb36a901f34ff5a7bd4105fc1efff70a96e0effa6\nsigner=425ed4e4a36b30ea\nsecurity_fix=0\n",
];

/// The root of the golden tree of three leaves, in hex.
pub const ROOT_HEX: &str = "0a7ec208d4a14cbe3b8b9bb769a4a444e70608827ba40fe38f41cef3a786b996";
/// The leaf hashes of the three records, in hex.
pub const LEAVES_HEX: [&str; 3] = [
    "a07d2c9ab9c9a7166ec00457983e844067966407e5eab42f6021b54136aec11f",
    "504edb2186cd428a9e3baec5896d6a3b257a3a2e626fcfed350de9824ca9f4df",
    "b96615cec237fc1f9e8d4f9ecc9f1aa2f0731f0c41dc8648df07c885bf5d4b45",
];
/// The checkpoint note of size 3 with the log's signature and the cosignatures of w1 and w2.
pub const NOTE: &str = "azlin.io/release-log/v1\n3\nCn7CCNShTL47i5u3aaSkROcGCIJ7pA/jj0HO86eGuZY=\n\n— azlin.io/release-log nVUlOzjeU6T992QJvBjkxBc3yvr+m3HtLE3plKQtmTDUV5fffHH1PZvh6tDguX2HJiZYsT+/nLr3194i7nzyHi+oYQw=\n— witness.example/w1 JLXZ/gAAAABlU/EAzdrzwN3UHI6siolp5yjVi50dN9/n72YHA+0Wlj5As7uMvms3R0ASe5WnS+natT8j1La2ZG1n9iS7Q5L4q5eJAg==\n— witness.example/w2 70DHpgAAAABlU/FkZtqABYA3y5+8IwVDk8OCMhuU9CPF3QZgACbc7xfj4eHCr8kwu4C1T1ZC/pk1jQwm4CEN0DFV7cNJpxys2/wLBg==\n";
/// The proof bundle of leaf 1 under that note.
pub const BUNDLE: &str = "azlin-proof-v1\nleaf=1\nsize=3\npath=oH0smrnJpxZuwARXmD6EQGeWZAfl6rQvYCG1QTauwR8=\npath=uWYVzsI3/B+ejU+ezJ8aovBzHwxB3IZI3wfIhb9dS0U=\nrecord=YXpsaW4tcmVsZWFzZS12MQpraW5kPXdvcmtlcgpuYW1lPWF6bWFpbC13b3JrZXIKdmVyc2lvbj0yLjAuMQpjaGFubmVsPXJlbGVhc2UKYXJjaD1hbnkKZGlnZXN0PXNoYTI1NjpjYTQ3NjA4YThkNTE1Njg4NTA3OGY2Y2IwZmNhZjFlYmIwMjU1NmU2N2I3NDU1MTYyZWFkMjdlNDFjZjJkYWMxCnNpZ25lcj00MjVlZDRlNGEzNmIzMGVhCnNlY3VyaXR5X2ZpeD0xCg==\ncheckpoint=YXpsaW4uaW8vcmVsZWFzZS1sb2cvdjEKMwpDbjdDQ05TaFRMNDdpNXUzYWFTa1JPY0dDSUo3cEEvamowSE84NmVHdVpZPQoK4oCUIGF6bGluLmlvL3JlbGVhc2UtbG9nIG5WVWxPemplVTZUOTkyUUp2QmpreEJjM3l2cittM0h0TEUzcGxLUXRtVERVVjVmZmZISDFQWnZoNnREZ3VYMkhKaVpZc1QrL25McjMxOTRpN256eUhpK29ZUXc9CuKAlCB3aXRuZXNzLmV4YW1wbGUvdzEgSkxYWi9nQUFBQUJsVS9FQXpkcnp3TjNVSEk2c2lvbHA1eWpWaTUwZE45L243MllIQSswV2xqNUFzN3VNdm1zM1IwQVNlNVduUytuYXRUOGoxTGEyWkcxbjlpUzdRNUw0cTVlSkFnPT0K4oCUIHdpdG5lc3MuZXhhbXBsZS93MiA3MERIcGdBQUFBQmxVL0ZrWnRxQUJZQTN5NSs4SXdWRGs4T0NNaHVVOUNQRjNRWmdBQ2JjN3hmajRlSENyOGt3dTRDMVQxWkMvcGsxalF3bTRDRU4wREZWN2NOSnB4eXMyL3dMQmc9PQo=\n";
/// The inclusion path of leaf 1, in hex, leaf side first.
pub const PATH_HEX: [&str; 2] = [
    "a07d2c9ab9c9a7166ec00457983e844067966407e5eab42f6021b54136aec11f",
    "b96615cec237fc1f9e8d4f9ecc9f1aa2f0731f0c41dc8648df07c885bf5d4b45",
];
/// The cosignature times of w1 and w2.
pub const TIME_W1: u64 = 1700000000;
pub const TIME_W2: u64 = 1700000100;
/// The public keys (hex) of the log key (seed 7), w1 (seed 9) and w2 (seed 11).
pub const LOG_PUBLIC: &str = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
pub const W1_PUBLIC: &str = "fd1724385aa0c75b64fb78cd602fa1d991fdebf76b13c58ed702eac835e9f618";
pub const W2_PUBLIC: &str = "66be7e332c7a453332bd9d0a7f7db055f5c5ef1a06ada66d98b39fb6810c473a";
