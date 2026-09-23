//! Retium addresses.
//!
//! An address is derived from a public key and carries a checksum, so a typo or
//! a truncated paste is refused instead of being paid to:
//!
//! ```text
//! rete ‖ hex(H) ‖ hex(C)      72 characters
//!   H = blake3(public key)            32 bytes
//!   C = blake3("retium.addr.v1" ‖ H)   first 2 bytes
//! ```
//!
//! Reserved ledger accounts (`rete_treasury`, `rete_burn`) are not addresses:
//! they never pass [`is_valid`], so they can never be used as a sender.

use crate::key::PublicKey;

/// Every address starts with this.
pub const PREFIX: &str = "rete";
/// prefix + 64 hex (hash) + 4 hex (checksum).
pub const LEN: usize = 4 + 64 + 4;

const CHECKSUM_DOMAIN: &[u8] = b"retium.addr.v1";

/// The address for a 32-byte account hash.
pub fn from_hash(hash: &[u8; 32]) -> String {
    let mut c = blake3::Hasher::new();
    c.update(CHECKSUM_DOMAIN);
    c.update(hash);
    let checksum = c.finalize();
    format!(
        "{}{}{}",
        PREFIX,
        hex::encode(hash),
        hex::encode(&checksum.as_bytes()[..2])
    )
}

/// The address of a public key.
pub fn from_public_key(key: &PublicKey) -> String {
    from_hash(blake3::hash(key.as_bytes()).as_bytes())
}

/// Is `s` a well-formed Retium address — right length and prefix, lowercase
/// hex, checksum intact? Check every address a user typed or pasted before you
/// put it in a transaction.
pub fn is_valid(s: &str) -> bool {
    if s.len() != LEN || !s.starts_with(PREFIX) {
        return false;
    }
    let body = &s[PREFIX.len()..];
    if !body
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return false;
    }
    let mut hash = [0u8; 32];
    if hex::decode_to_slice(&body[..64], &mut hash).is_err() {
        return false;
    }
    from_hash(&hash) == s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr() -> String {
        from_hash(&[7u8; 32])
    }

    #[test]
    fn an_address_is_72_characters_and_valid() {
        let a = addr();
        assert_eq!(a.len(), LEN);
        assert!(a.starts_with(PREFIX));
        assert!(is_valid(&a));
    }

    #[test]
    fn a_typo_is_refused() {
        let a = addr();
        let mut typo = a.clone();
        typo.replace_range(10..11, if &a[10..11] == "a" { "b" } else { "a" });
        assert!(
            !is_valid(&typo),
            "a one-character change must fail the checksum"
        );
    }

    #[test]
    fn truncated_padded_and_uppercase_are_refused() {
        let a = addr();
        assert!(!is_valid(&a[..a.len() - 1]));
        assert!(!is_valid(&format!("{} ", a)));
        assert!(!is_valid(&a.to_uppercase()));
    }

    #[test]
    fn reserved_accounts_are_not_addresses() {
        assert!(!is_valid("rete_treasury"));
        assert!(!is_valid("rete_burn"));
    }
}
