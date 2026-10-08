//! Calling another contract.
//!
//! A cross-contract call runs inside your call. If the other contract traps,
//! runs out of fuel or is refused by the chain, everything it did is undone and
//! you get an `Err` back. If you then return success your own writes still
//! stand, so decide deliberately which of those you want.
//!
//! A contract that answers with an error reply has not failed in that sense:
//! it ran to the end, its writes stay, and you get its reply as `Ok`. Check the
//! reply with [`is_error_reply`] whenever the call must have worked.
//!
//! Three rules the chain enforces:
//!
//! * a contract cannot call itself, directly or in a circle;
//! * calls may nest at most 64 deep;
//! * a call is expensive in fuel, so do not put one in a loop over user input.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;

use crate::events::json_string;
use crate::host;
use crate::{Error, MAX_OUTPUT};

/// The chain's own RTM ledger, callable like a contract.
pub const RTM: &str = "RTM_NATIVE";

/// Call `method` on another contract.
pub fn contract(contract_id: &str, method: &str, args: &[&str]) -> Result<String, Error> {
    let args = encode_args(args);
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
    let args = encode_args(args);
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
/// the caller's wallet: what they attach to a call is all you get.
pub fn send_rtm(to: &str, amount: u128) -> Result<String, Error> {
    contract(RTM, "transfer", &[to, &amount.to_string()])
}

/// Whether a reply from another contract reports an error: a JSON object with
/// a top-level `error` field, which is what [`error`](crate::error) produces.
///
/// A plain value such as an RTM balance, or `error` appearing inside some other
/// field's text, is not an error reply.
pub fn is_error_reply(reply: &str) -> bool {
    crate::json::Object::parse(reply)
        .map(|object| object.has("error"))
        .unwrap_or(false)
}

/// Arguments as the chain reads them: a JSON array of strings.
fn encode_args(args: &[&str]) -> String {
    let mut out = String::from("[");
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_string(arg));
    }
    out.push(']');
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn arguments_reach_the_chain_as_it_reads_them() {
        // The chain reads the arguments of a cross-contract call as a JSON
        // array of strings and refuses the call when they are anything else.
        let cases: [&[&str]; 6] = [
            &[],
            &["rete-bob", "10"],
            &[""],
            &["a,b", "c"],
            &["Pepe \"The Frog\"", "a\\b", "x]y", "]"],
            &["line\nnext\ttab", "\u{1}\u{1f}", "é中😀"],
        ];
        for args in cases {
            let sent = encode_args(args);
            let read: Vec<String> = serde_json::from_str(&sent)
                .unwrap_or_else(|e| panic!("the chain would refuse {sent}: {e}"));
            assert_eq!(read, args, "{sent}");
        }
        assert_eq!(encode_args(&[]), "[]");
        assert_eq!(encode_args(&["rete-bob", "10"]), r#"["rete-bob","10"]"#);
    }

    #[test]
    fn an_error_reply_is_told_from_an_answer() {
        assert!(is_error_reply(r#"{"ok":false,"error":"not enough"}"#));
        assert!(is_error_reply(r#"{"error":"not enough"}"#));
        assert!(is_error_reply(&format!(
            "{{\"ok\":false,\"error\":{}}}",
            json_string("say \"no\"")
        )));
        assert!(!is_error_reply(&crate::ok("done").unwrap()));
        assert!(!is_error_reply(
            &crate::ok("the word \"error\" in an answer").unwrap()
        ));
        assert!(!is_error_reply(r#"{"ok":true,"data":{"error":"nested"}}"#));
        assert!(!is_error_reply("1500"));
        assert!(!is_error_reply("true"));
        assert!(!is_error_reply(""));
    }
}
