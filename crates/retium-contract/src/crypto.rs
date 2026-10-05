//! Hashing and signature checking inside a contract.

use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::host;
use crate::Error;

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> Result<[u8; 32], Error> {
    let mut out = [0u8; 32];
    let rc = unsafe { host::sha256(data.as_ptr(), data.len() as i32, out.as_mut_ptr()) };
    if rc != 32 {
        return Err(String::from("sha256 failed"));
    }
    Ok(out)
}

/// SHA-256 as lowercase hex.
pub fn sha256_hex(data: &[u8]) -> Result<String, Error> {
    let digest = sha256(data)?;
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    Ok(out)
}

/// Check an ML-DSA-44 signature: the same scheme Retium accounts use.
///
/// This is one of the most expensive things a contract can do. Use it only when
/// you genuinely need to verify something signed off-chain — a call's `caller`
/// is already proven by the chain.
pub fn verify_signature(
    message: &[u8],
    signature: &[u8],
    public_key: &[u8],
) -> Result<bool, Error> {
    let rc = unsafe {
        host::sig_verify(
            message.as_ptr(),
            message.len() as i32,
            signature.as_ptr(),
            signature.len() as i32,
            public_key.as_ptr(),
            public_key.len() as i32,
        )
    };
    match rc {
        1 => Ok(true),
        0 => Ok(false),
        _ => Err(String::from(
            "signature check failed: wrong signature or key length",
        )),
    }
}

/// Decode lowercase or uppercase hex.
pub fn from_hex(text: &str) -> Result<alloc::vec::Vec<u8>, Error> {
    if text.len() % 2 != 0 {
        return Err(String::from("hex must have an even number of characters"));
    }
    let bytes = text.as_bytes();
    let mut out = vec![0u8; text.len() / 2];
    for (i, pair) in bytes.chunks(2).enumerate() {
        let hi = digit(pair[0])?;
        let lo = digit(pair[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn digit(c: u8) -> Result<u8, Error> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("'{}' is not a hex digit", c as char)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decodes_both_cases_and_refuses_junk() {
        assert_eq!(from_hex("00ff10").unwrap(), [0x00, 0xff, 0x10]);
        assert_eq!(from_hex("00FF").unwrap(), [0x00, 0xff]);
        assert!(from_hex("abc").is_err(), "odd length");
        assert!(from_hex("zz").is_err(), "not hex");
        assert!(from_hex("").unwrap().is_empty());
    }
}
