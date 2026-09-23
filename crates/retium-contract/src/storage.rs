//! Contract storage: the only place state survives between calls.
//!
//! Storage belongs to your contract; no other contract can write it. Keys are
//! strings you choose, values are strings up to [`MAX_VALUE`](crate::MAX_VALUE)
//! bytes. Writing an empty value deletes the key.
//!
//! Storage is permanent and costs fuel by size. Store what you need, not
//! everything you have.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::host;
use crate::{Error, MAX_VALUE};

/// Longest value a single read will return. Ask for more by chunking your keys.
const READ_BUFFER: usize = MAX_VALUE;

/// Store `value` under `key`. An empty value deletes the key.
pub fn set(key: &str, value: &str) -> Result<(), Error> {
    if key.is_empty() {
        return Err("storage key must not be empty".to_string());
    }
    if value.len() > MAX_VALUE {
        return Err(format!(
            "value is {} bytes; the limit is {MAX_VALUE}",
            value.len()
        ));
    }
    unsafe {
        host::write_state(
            key.as_ptr(),
            key.len() as i32,
            value.as_ptr(),
            value.len() as i32,
        );
    }
    Ok(())
}

/// Read `key`. `None` means the key has never been set, or was deleted.
pub fn get(key: &str) -> Result<Option<String>, Error> {
    if key.is_empty() {
        return Err("storage key must not be empty".to_string());
    }
    let mut buf = vec![0u8; READ_BUFFER];
    let written = unsafe {
        host::read_state(
            key.as_ptr(),
            key.len() as i32,
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    match written {
        0 => Ok(None),
        n if n < 0 => Err(format!("could not read '{key}'")),
        n => {
            buf.truncate(n as usize);
            String::from_utf8(buf)
                .map(Some)
                .map_err(|_| format!("value of '{key}' is not UTF-8"))
        }
    }
}

/// Read `key`, or a default when it is not set.
pub fn get_or(key: &str, default: &str) -> Result<String, Error> {
    Ok(get(key)?.unwrap_or_else(|| default.to_string()))
}

/// Read `key` as a whole number, treating "not set" as 0.
pub fn get_u128(key: &str) -> Result<u128, Error> {
    match get(key)? {
        None => Ok(0),
        Some(raw) => raw
            .parse()
            .map_err(|_| format!("value of '{key}' is not a number: '{raw}'")),
    }
}

/// Store a whole number.
pub fn set_u128(key: &str, value: u128) -> Result<(), Error> {
    set(key, &value.to_string())
}

/// Add to a stored number, refusing to overflow.
pub fn add_u128(key: &str, delta: u128) -> Result<u128, Error> {
    let next = get_u128(key)?
        .checked_add(delta)
        .ok_or_else(|| format!("'{key}' would overflow"))?;
    set_u128(key, next)?;
    Ok(next)
}

/// Subtract from a stored number, refusing to go below zero.
pub fn sub_u128(key: &str, delta: u128) -> Result<u128, Error> {
    let current = get_u128(key)?;
    let next = current
        .checked_sub(delta)
        .ok_or_else(|| format!("'{key}' has {current}, cannot take {delta}"))?;
    set_u128(key, next)?;
    Ok(next)
}

/// Delete `key`.
pub fn remove(key: &str) -> Result<(), Error> {
    set(key, "")
}

/// Keys that start with `prefix`, as the chain has them.
///
/// Costs fuel per key returned — use narrow prefixes, and never assume the list
/// is small.
pub fn keys_with_prefix(prefix: &str) -> Result<Vec<String>, Error> {
    let mut buf = vec![0u8; crate::MAX_OUTPUT];
    let written = unsafe {
        host::list_keys(
            prefix.as_ptr(),
            prefix.len() as i32,
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    if written < 0 {
        return Err(format!("could not list keys under '{prefix}'"));
    }
    buf.truncate(written as usize);
    let text = String::from_utf8(buf).map_err(|_| "key list was not UTF-8".to_string())?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(crate::json::Object::parse(&format!("{{\"keys\":{text}}}"))
        .map(|o| o.string_array("keys"))
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_keys_are_refused() {
        assert!(set("", "x").is_err());
        assert!(get("").is_err());
    }

    #[test]
    fn oversized_values_are_refused_before_the_host_sees_them() {
        let big = "x".repeat(MAX_VALUE + 1);
        let err = set("k", &big).unwrap_err();
        assert!(err.contains("limit"), "{err}");
    }

    #[test]
    fn numbers_refuse_to_wrap_around() {
        // Off-chain, reads return "not set", so these exercise the arithmetic
        // guards rather than the host.
        assert_eq!(get_u128("counter").unwrap(), 0);
        assert!(sub_u128("counter", 1).is_err(), "cannot go below zero");
    }
}
