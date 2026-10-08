//! What a contract can learn about where it is running.
//!
//! There is no wall clock and no randomness here, on purpose: every keeper runs
//! your code and must reach the same answer. Block height and block time are
//! agreed by the chain, so they are safe to use; the system clock of whichever
//! machine ran you is not available at all.

use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::host;
use crate::Error;

/// This contract's own id.
pub fn contract_id() -> Result<String, Error> {
    read_string(
        |out, max| unsafe { host::get_self_contract_id(out, max) },
        128,
        "contract id",
    )
}

/// The height of the block this call is in.
pub fn block_height() -> i64 {
    unsafe { host::get_block_height() }
}

/// The block's timestamp in milliseconds, agreed by the chain, not read from a
/// machine's clock.
pub fn block_time_ms() -> i64 {
    unsafe { host::get_block_time() }
}

/// The code hash of a deployed contract: what it is, not what it says it is.
pub fn code_hash_of(contract: &str) -> Result<String, Error> {
    read_string(
        |out, max| unsafe {
            host::get_contract_code_hash(contract.as_ptr(), contract.len() as i32, out, max)
        },
        64,
        "code hash",
    )
}

/// The version another contract declares.
pub fn version_of(contract: &str) -> Result<String, Error> {
    read_string(
        |out, max| unsafe {
            host::get_contract_version(contract.as_ptr(), contract.len() as i32, out, max)
        },
        64,
        "version",
    )
}

/// Declare this contract's version. At most 64 bytes.
pub fn set_version(version: &str) -> Result<(), Error> {
    if version.is_empty() || version.len() > 64 {
        return Err(format!("version must be 1–64 bytes, got {}", version.len()));
    }
    let rc = unsafe { host::set_contract_version(version.as_ptr(), version.len() as i32) };
    if rc < 0 {
        return Err(String::from("could not set version"));
    }
    Ok(())
}

/// Does another contract claim a capability?
///
/// This is what that contract's deployer declared, not something the chain
/// checks. Treat it as a hint, never as permission.
pub fn claims_capability(contract: &str, capability: &str) -> bool {
    let rc = unsafe {
        host::supports_capability(
            contract.as_ptr(),
            contract.len() as i32,
            capability.as_ptr(),
            capability.len() as i32,
        )
    };
    rc == 1
}

fn read_string(
    call: impl Fn(*mut u8, i32) -> i32,
    capacity: usize,
    what: &str,
) -> Result<String, Error> {
    let mut buf = vec![0u8; capacity];
    let written = call(buf.as_mut_ptr(), buf.len() as i32);
    if written < 0 {
        return Err(format!("could not read {what}"));
    }
    buf.truncate(written as usize);
    String::from_utf8(buf).map_err(|_| format!("{what} was not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_must_fit_the_chains_limit() {
        assert!(set_version("").is_err());
        assert!(set_version(&"v".repeat(65)).is_err());
    }
}
