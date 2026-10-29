//! Native random numbers.
//!
//! A contract asks the chain for a number with [`request`]. The number arrives
//! in a second call, with action [`CALLBACK`], after the transaction that asked
//! is final. [`callback`] checks that call and unpacks it.
//!
//! ```ignore
//! "roll" => {
//!     let id = random::request()?;
//!     storage::set(&format!("bet:{id}"), call.caller())?;
//!     ok(&id)
//! }
//! random::CALLBACK => {
//!     let (id, number) = random::callback(&call)?;
//!     let face = random::in_range(&number, 6) + 1;
//!     // settle the bet stored under `id`
//!     ok(&face.to_string())
//! }
//! ```
//!
//! * Take every bet in the transaction that asks. The request id is that
//!   transaction's id.
//! * Settle only through [`callback`]. It refuses any caller but the chain.
//! * The callback cannot ask again.
//!
//! The callback is a call of its own, after your transaction, with the ordinary
//! fuel budget and no gas for the sender. If it fails, the number still exists
//! on chain and the request is closed.
//!
//! A keeper reveals a secret, one per request, after the transaction is final,
//! and the number is derived from that secret and the request id. The reveal
//! is at `GET /api/vrf/request/{tx_id}`. The keeper that reveals sees the number
//! before everyone else and can only refuse to reveal, which is visible on
//! chain.

use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::host;
use crate::{Call, Error};

/// The action the chain calls your contract with to deliver a number.
pub const CALLBACK: &str = "on_random";

/// The caller the chain uses for that call. Nobody else can be it.
pub const CALLER: &str = "retium-vrf";

/// Ask the chain for a random number.
///
/// Returns the request id, which is this transaction's id. The number arrives
/// later in a call with action [`CALLBACK`]. Costs 50,000 fuel, like a
/// cross-contract call.
///
/// Fails in a free read (a view call), inside the callback itself, and when
/// another contract in the same transaction asked first. Asking twice in one
/// transaction returns the same id.
pub fn request() -> Result<String, Error> {
    let mut out = vec![0u8; 128];
    let written = unsafe { host::request_random(out.as_mut_ptr(), out.len() as i32) };
    if written < 0 {
        return Err(String::from(
            "random number not available here (view call, inside on_random, or already requested)",
        ));
    }
    out.truncate(written as usize);
    String::from_utf8(out).map_err(|_| String::from("request id was not UTF-8"))
}

/// Check that `call` is the chain delivering a number and unpack it as
/// `(request_id, number)`.
///
/// Refuses any caller other than the chain. A user can send a call named
/// `on_random` too.
pub fn callback(call: &Call) -> Result<(String, [u8; 32]), Error> {
    if call.caller() != CALLER {
        return Err(format!("{CALLBACK} may only be called by the chain"));
    }
    let id = call.arg(0)?;
    let hex = call.arg(1)?;
    let bytes = crate::crypto::from_hex(hex)?;
    if bytes.len() != 32 {
        return Err(format!(
            "random number must be 32 bytes, got {}",
            bytes.len()
        ));
    }
    let mut number = [0u8; 32];
    number.copy_from_slice(&bytes);
    Ok((String::from(id), number))
}

/// Reduce a number to the range `0..n`.
///
/// Uses the first 16 bytes, so the bias for any practical `n` is below one in
/// 2^100. `n` of 0 gives 0.
pub fn in_range(number: &[u8; 32], n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut head = [0u8; 16];
    head.copy_from_slice(&number[..16]);
    (u128::from_be_bytes(head) % (n as u128)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(caller: &str, args: &[&str]) -> Call {
        let args = args
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<alloc::vec::Vec<_>>()
            .join(",");
        Call::from_json(&format!(
            r#"{{"action":"on_random","caller":"{caller}","args":[{args}],"value":"0"}}"#
        ))
        .unwrap()
    }

    const NUMBER: &str = "f50b348181a1616a24e40e4a0dd395332973bb33d50cb40cc70352c0ff78ce31";

    #[test]
    fn only_the_chain_may_deliver_a_number() {
        assert!(callback(&call("rete-mallory", &["tx_1", NUMBER])).is_err());
        let (id, number) = callback(&call(CALLER, &["tx_1", NUMBER])).unwrap();
        assert_eq!(id, "tx_1");
        assert_eq!(number[0], 0xf5);
        assert_eq!(number[31], 0x31);
    }

    #[test]
    fn a_number_of_the_wrong_size_is_refused() {
        assert!(callback(&call(CALLER, &["tx_1", "f50b"])).is_err());
        assert!(callback(&call(CALLER, &["tx_1", "zz"])).is_err());
        assert!(callback(&call(CALLER, &["tx_1"])).is_err());
    }

    #[test]
    fn in_range_reads_the_leading_bytes() {
        let (_, number) = callback(&call(CALLER, &["tx_1", NUMBER])).unwrap();
        assert_eq!(in_range(&number, 1), 0);
        assert_eq!(in_range(&number, 0), 0);
        assert!(in_range(&number, 6) < 6);
        // The face a local run of the dice example saw for this number.
        assert_eq!(in_range(&number, 6) + 1, 6);
    }

    #[test]
    fn a_request_off_chain_reports_no_chain() {
        assert!(request().is_err());
    }
}
