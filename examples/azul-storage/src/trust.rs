//! The certificate authorities the Azlin apps pin: the iroh relays (n0's and the Azlin nodes')
//! are served under Let's Encrypt, so a relay's TLS trusts Let's Encrypt's four roots and
//! nothing else of the ~150 public ones. A forged relay certificate from any other authority is
//! refused in the handshake. The token server's and the nodes' HTTPS keep the public roots for
//! now: their edge certificates rotate among several authorities.

/// Let's Encrypt's roots, by name: X1 (RSA), X2 (ECDSA) and the 2025 generation's YE and YR
/// (n0's relays chain to Root YE, cross-signed by X2). PEM text from letsencrypt.org.
pub const AZLIN_RELAY_ROOTS: [(&str, &str); 4] = [
    ("ISRG Root X1", include_str!("../roots/ISRGRootX1.pem")),
    ("ISRG Root X2", include_str!("../roots/ISRGRootX2.pem")),
    ("ISRG Root YE", include_str!("../roots/ISRGRootYE.pem")),
    ("ISRG Root YR", include_str!("../roots/ISRGRootYR.pem")),
];
