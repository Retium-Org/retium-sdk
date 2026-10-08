//! Keys and signatures: ML-DSA-44 (FIPS 204), the only scheme Retium accepts.
//!
//! A key file is 37 bytes: `RTSK` ‖ version ‖ 32-byte seed. The seed is the
//! secret; the public key and the address are derived from it, so a backup of
//! those 32 bytes is a complete backup of the account.
//!
//! The secret never leaves this process: signing is local, and [`SecretKey`]
//! wipes its seed from memory when dropped.

use ml_dsa::{Keypair as _, MlDsa44, Signer as _, Verifier as _};
use zeroize::Zeroize;

use crate::error::{Error, Result};

type Params = MlDsa44;

/// Signature scheme version, and the first byte of every signature and key file.
pub const VERSION: u8 = 2;
/// Length of the secret seed.
pub const SEED_LEN: usize = 32;
/// Length of an encoded public key.
pub const PUBLIC_KEY_LEN: usize = 1312;
/// Length of a signature: version byte + ML-DSA-44 signature.
pub const SIGNATURE_LEN: usize = 1 + 2420;
/// Key files start with these four bytes.
pub const KEY_FILE_MAGIC: &[u8; 4] = b"RTSK";
/// Length of a key file: magic + version + seed.
pub const KEY_FILE_LEN: usize = 4 + 1 + SEED_LEN;

/// A public key. Safe to publish; an account's address is derived from it.
#[derive(Clone)]
pub struct PublicKey {
    encoded: Vec<u8>,
    inner: ml_dsa::VerifyingKey<Params>,
}

impl PublicKey {
    /// Parse the 1,312 encoded bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != PUBLIC_KEY_LEN {
            return Err(Error::BadLength {
                what: "public key",
                expected: PUBLIC_KEY_LEN,
                got: bytes.len(),
            });
        }
        let encoded_key = ml_dsa::EncodedVerifyingKey::<Params>::try_from(bytes)
            .map_err(|_| Error::BadEncoding("public key"))?;
        Ok(Self {
            encoded: bytes.to_vec(),
            inner: ml_dsa::VerifyingKey::<Params>::decode(&encoded_key),
        })
    }

    /// Parse hex, with or without a `0x` prefix.
    pub fn from_hex(s: &str) -> Result<Self> {
        let s = s.trim();
        let s = s.strip_prefix("0x").unwrap_or(s);
        Self::from_bytes(&hex::decode(s).map_err(|_| Error::BadEncoding("public key hex"))?)
    }

    /// The encoded key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.encoded
    }

    /// The encoded key as hex, the form RPC requests carry.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.encoded)
    }

    /// This key's Retium address.
    pub fn address(&self) -> String {
        crate::address::from_public_key(self)
    }

    /// Check a signature made by the matching secret key.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        if signature.len() != SIGNATURE_LEN || signature[0] != VERSION {
            return false;
        }
        match ml_dsa::Signature::<Params>::try_from(&signature[1..]) {
            Ok(sig) => self.inner.verify(message, &sig).is_ok(),
            Err(_) => false,
        }
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.encoded == other.encoded
    }
}

impl Eq for PublicKey {}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicKey({}…)", &self.to_hex()[..16])
    }
}

/// A secret key. Keep it out of logs, arguments and version control.
///
/// The seed is wiped from memory when this value is dropped.
pub struct SecretKey {
    seed: [u8; SEED_LEN],
    inner: ml_dsa::SigningKey<Params>,
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

impl SecretKey {
    /// A new random key from the operating system's randomness.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut seed = [0u8; SEED_LEN];
        rand::rngs::OsRng.fill_bytes(&mut seed);
        let key = Self::from_seed(&seed);
        seed.zeroize();
        key
    }

    /// Rebuild a key from its 32-byte seed (FIPS 204 key generation).
    pub fn from_seed(seed: &[u8; SEED_LEN]) -> Self {
        let xi = ml_dsa::B32::try_from(&seed[..]).expect("SEED_LEN is 32");
        Self {
            seed: *seed,
            inner: ml_dsa::SigningKey::<Params>::from_seed(&xi),
        }
    }

    /// Rebuild a key from a 64-character hex seed.
    pub fn from_hex_seed(s: &str) -> Result<Self> {
        let s = s.trim();
        let s = s.strip_prefix("0x").unwrap_or(s);
        let mut seed = [0u8; SEED_LEN];
        hex::decode_to_slice(s, &mut seed).map_err(|_| Error::BadEncoding("seed hex"))?;
        let key = Self::from_seed(&seed);
        seed.zeroize();
        Ok(key)
    }

    /// Parse a 37-byte key file. A pre-2026 32-byte file is refused by name.
    pub fn from_key_file_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() == SEED_LEN {
            return Err(Error::LegacyKeyFile);
        }
        if bytes.len() != KEY_FILE_LEN {
            return Err(Error::BadLength {
                what: "key file",
                expected: KEY_FILE_LEN,
                got: bytes.len(),
            });
        }
        if &bytes[..4] != KEY_FILE_MAGIC {
            return Err(Error::BadEncoding("key file magic (want RTSK)"));
        }
        if bytes[4] != VERSION {
            return Err(Error::BadEncoding("key file version"));
        }
        let mut seed = [0u8; SEED_LEN];
        seed.copy_from_slice(&bytes[5..]);
        let key = Self::from_seed(&seed);
        seed.zeroize();
        Ok(key)
    }

    /// Read a key file from disk.
    pub fn read_key_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes =
            std::fs::read(path).map_err(|e| Error::Io(format!("{}: {}", path.display(), e)))?;
        Self::from_key_file_bytes(&bytes)
    }

    /// The 37 bytes to write to a key file.
    pub fn to_key_file_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(KEY_FILE_LEN);
        out.extend_from_slice(KEY_FILE_MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.seed);
        out
    }

    /// Write a key file, readable only by its owner on Unix. Refuses to
    /// overwrite: a key file is not something to replace by accident.
    pub fn write_key_file(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        if path.exists() {
            return Err(Error::Io(format!("{} already exists", path.display())));
        }
        std::fs::write(path, self.to_key_file_bytes())
            .map_err(|e| Error::Io(format!("{}: {}", path.display(), e)))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// The seed, for backup. Handle it like the secret it is.
    pub fn seed_hex(&self) -> String {
        hex::encode(self.seed)
    }

    /// The matching public key.
    pub fn public_key(&self) -> PublicKey {
        let inner = self.inner.verifying_key();
        let encoded = inner.encode()[..].to_vec();
        PublicKey { encoded, inner }
    }

    /// This key's Retium address.
    pub fn address(&self) -> String {
        self.public_key().address()
    }

    /// Sign a message. Retium signs the 32-byte canonical hash of a
    /// transaction; see [`crate::canonical`].
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let sig = self.inner.sign(message);
        let mut out = Vec::with_capacity(SIGNATURE_LEN);
        out.push(VERSION);
        out.extend_from_slice(&sig.encode()[..]);
        out
    }

    /// Signature as hex, the form every Retium RPC call expects.
    pub fn sign_hex(&self, message: &[u8]) -> String {
        hex::encode(self.sign(message))
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the seed.
        write!(f, "SecretKey(<redacted>, {})", self.address())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_always_rebuilds_the_same_account() {
        let a = SecretKey::from_seed(&[9u8; SEED_LEN]);
        let b = SecretKey::from_hex_seed(&a.seed_hex()).unwrap();
        assert_eq!(a.address(), b.address());
        assert_eq!(a.public_key().to_hex(), b.public_key().to_hex());
        assert!(crate::address::is_valid(&a.address()));
    }

    #[test]
    fn a_key_file_round_trips() {
        let key = SecretKey::from_seed(&[1u8; SEED_LEN]);
        let bytes = key.to_key_file_bytes();
        assert_eq!(bytes.len(), KEY_FILE_LEN);
        assert_eq!(&bytes[..4], KEY_FILE_MAGIC);
        let back = SecretKey::from_key_file_bytes(&bytes).unwrap();
        assert_eq!(back.address(), key.address());
    }

    #[test]
    fn a_pre_2026_key_file_is_refused_by_name() {
        let err = SecretKey::from_key_file_bytes(&[0u8; 32]).unwrap_err();
        assert!(matches!(err, Error::LegacyKeyFile), "got {err}");
    }

    #[test]
    fn signatures_verify_and_tampering_does_not() {
        let key = SecretKey::from_seed(&[5u8; SEED_LEN]);
        let pk = key.public_key();
        let sig = key.sign(b"hello");
        assert_eq!(sig.len(), SIGNATURE_LEN);
        assert_eq!(sig[0], VERSION);
        assert!(pk.verify(b"hello", &sig));
        assert!(!pk.verify(b"hell0", &sig), "different message");
        let other = SecretKey::from_seed(&[6u8; SEED_LEN]).public_key();
        assert!(!other.verify(b"hello", &sig), "different key");
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let key = SecretKey::from_seed(&[3u8; SEED_LEN]);
        let shown = format!("{key:?}");
        assert!(!shown.contains(&key.seed_hex()));
        assert!(shown.contains("redacted"));
    }
}
