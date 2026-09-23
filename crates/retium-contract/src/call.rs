//! Calling another contract.
//!
//! A cross-contract call runs inside your call: if it fails, its writes are
//! undone and you get an error back, and if you then return success your own
//! writes still stand. Decide deliberately which of those you want.
//!
//! Three rules the chain enforces:
//!
//! * a contract cannot call itself, directly or in a circle;
//! * calls may nest at most 64 deep;
//! * a call is expensive in fuel, so do not put one in a loop over user input.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;

use crate::host;
use crate::{Error, MAX_OUTPUT};

/// The chain's own RTM ledger, callable like a contract.
pub const RTM: &str = "RTM_NATIVE";

/// Call `method` on another contract.
pub fn contract(contract_id: &str, method: &str, args: &[&str]) -> Result<String, Error> {
    let args = args.join(",");
    let mut out = vec![0u8; MAX_OUTPUT];
    let written = unsafe {
        host::call_contract(
            contract_id.as_ptr(),
            contract_id.len() as i32,
            method.as_ptr(),
            method.len() as i32,
            args.as_ptr(),
            args.len() as i32,
            out.as_mut_ptr(),
            out.len() as i32,
        )
    };
    finish(written, out, contract_id, method)
}

/// Call `method` on another contract, attaching `amount` atomic units of RTM
/// from this contract's own balance.
///
/// If the call fails, the money comes back with it.
pub fn contract_with_value(
    contract_id: &str,
    method: &str,
    args: &[&str],
    amount: u128,
) -> Result<String, Error> {
    let args = args.join(",");
    let amount = amount.to_string();
    let mut out = vec![0u8; MAX_OUTPUT];
    let written = unsafe {
        host::call_contract_value(
            contract_id.as_ptr(),
            contract_id.len() as i32,
            method.as_ptr(),
            method.len() as i32,
            args.as_ptr(),
            args.len() as i32,
            amount.as_ptr(),
            amount.len() as i32,
            out.as_mut_ptr(),
            out.len() as i32,
        )
    };
    finish(written, out, contract_id, method)
}

/// This contract's RTM balance, in atomic units.
pub fn rtm_balance_of(address: &str) -> Result<u128, Error> {
    let raw = contract(RTM, "balance_of", &[address])?;
    raw.trim()
        .trim_matches('"')
        .parse()
        .map_err(|_| format!("RTM balance was not a number: '{raw}'"))
}

/// Send RTM from this contract to `to`.
///
/// A contract can only ever move its own RTM. There is no way to take RTM from
/// the caller's wallet — what they attach to a call is all you get.
pub fn send_rtm(to: &str, amount: u128) -> Result<String, Error> {
    contract(RTM, "transfer", &[to, &amount.to_string()])
}

fn finish(
    written: i32,
    mut out: alloc::vec::Vec<u8>,
    contract_id: &str,
    method: &str,
) -> Result<String, Error> {
    if written < 0 {
        return Err(format!("call to {contract_id}.{method} failed"));
    }
    out.truncate(written as usize);
    String::from_utf8(out).map_err(|_| format!("{contract_id}.{method} returned invalid UTF-8"))
}
