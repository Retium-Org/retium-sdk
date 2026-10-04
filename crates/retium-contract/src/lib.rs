//! Write Retium smart contracts in Rust.
//!
//! A Retium contract is a wasm module with a single entry point. The chain
//! hands it a call, it does its work, and it returns JSON:
//!
//! ```ignore
//! use retium_contract::{entrypoint, error, ok, storage, Call, Reply};
//!
//! entrypoint!(handle);
//!
//! fn handle(call: Call) -> Reply {
//!     match call.action() {
//!         "set" => {
//!             storage::set(call.arg(0)?, call.arg(1)?)?;
//!             ok("stored")
//!         }
//!         "get" => ok(&storage::get(call.arg(0)?)?.unwrap_or_default()),
//!         other => error(&alloc::format!("unknown action: {other}")),
//!     }
//! }
//! ```
//!
//! # Five rules that decide whether your contract works
//!
//! 1. **Nothing in memory survives.** The module is rebuilt for every call, so
//!    globals and `static` are wiped. State lives in [`storage`] only.
//! 2. **Failure must trap to undo work.** Returning an error after you have
//!    written state leaves those writes in place. Check first, write second, or
//!    call [`fail`] to roll the whole call back.
//! 3. **Be deterministic.** Every keeper runs your code and must reach the same
//!    answer. No clocks, no randomness, no network, no floating point.
//! 4. **Respect the limits.** A reply is at most [`MAX_OUTPUT`] bytes, a stored
//!    value at most [`MAX_VALUE`], an event payload at most [`MAX_EVENT_DATA`].
//! 5. **You cannot reach into a caller's wallet.** The only RTM you receive is
//!    what the caller attached to the call ([`Call::value`]), and they signed
//!    that amount.
//!
//! # Fuel
//!
//! Work costs fuel, and a call that runs out fails *after* the sender has paid
//! gas. Storage is charged by size, cross-contract calls are expensive, and
//! signature checks are very expensive. Keep loops bounded.

#![cfg_attr(target_arch = "wasm32", no_std)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub mod call;
pub mod crypto;
pub mod env;
pub mod events;
pub(crate) mod host;
pub(crate) mod json;
pub mod storage;

#[cfg(target_arch = "wasm32")]
mod rt;

/// The largest reply a contract may return, in bytes.
pub const MAX_OUTPUT: usize = 4096;
/// The largest value a single storage key may hold, in bytes.
pub const MAX_VALUE: usize = 64 * 1024;
/// The largest event payload, in bytes.
pub const MAX_EVENT_DATA: usize = 4096;
/// The largest event type name, in bytes.
pub const MAX_EVENT_TYPE: usize = 64;

/// What went wrong inside a contract.
///
/// Returning an `Err` from a handler produces a JSON error reply; the call
/// still counts as executed, so any state you already wrote stays written. Use
/// [`fail`] when you need the call undone.
pub type Error = String;

/// The result of a handler.
pub type Reply = Result<String, Error>;

/// A successful reply carrying `data`.
///
/// The chain requires a reply to be valid JSON, so this wraps `data` as
/// `{"ok":true,"data":"…"}` with the value escaped for you.
pub fn ok(data: &str) -> Reply {
    Ok(format!(
        "{{\"ok\":true,\"data\":{}}}",
        events::json_string(data)
    ))
}

/// A failed reply carrying `message`.
///
/// The call still counts as executed and the sender still pays gas, so any
/// state written before this point stays written. Check first, write second —
/// or use [`fail`] to undo the whole call.
pub fn error(message: &str) -> Reply {
    Err(message.to_string())
}

/// Abandon the call and undo everything it has done.
///
/// This traps the module. The chain discards every write, event and transfer
/// the call made; the sender still pays gas and uses their nonce. It is the
/// only way to be certain a half-finished change does not stick.
pub fn fail(_reason: &str) -> ! {
    #[cfg(target_arch = "wasm32")]
    {
        core::arch::wasm32::unreachable()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        panic!("contract failed: {_reason}")
    }
}

/// One call into your contract.
pub struct Call {
    action: String,
    caller: String,
    args: Vec<String>,
    token_context: Option<String>,
    value: u128,
}

impl Call {
    /// The method being called.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Who signed this call. The chain proves this; you do not have to.
    pub fn caller(&self) -> &str {
        &self.caller
    }

    /// All arguments, in order.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Argument `index`, or an error naming what was missing.
    pub fn arg(&self, index: usize) -> Result<&str, Error> {
        self.args.get(index).map(String::as_str).ok_or_else(|| {
            format!(
                "missing argument {} for action '{}'",
                index + 1,
                self.action
            )
        })
    }

    /// Argument `index` parsed as a whole number.
    pub fn arg_u128(&self, index: usize) -> Result<u128, Error> {
        let raw = self.arg(index)?;
        raw.parse::<u128>()
            .map_err(|_| format!("argument {} must be a whole number, got '{raw}'", index + 1))
    }

    /// RTM attached to this call, in atomic units.
    ///
    /// This is the only money a caller can give you, and they signed it.
    pub fn value(&self) -> u128 {
        self.value
    }

    /// The token context the node passed, if any.
    pub fn token_context(&self) -> Option<&str> {
        self.token_context.as_deref()
    }

    /// Parse a call from the JSON the node writes into memory.
    ///
    /// Exposed so you can unit-test a handler on your own machine without a
    /// chain.
    pub fn from_json(input: &str) -> Result<Self, Error> {
        let obj = json::Object::parse(input)?;
        Ok(Self {
            action: obj.string("action").unwrap_or_default(),
            caller: obj.string("caller").unwrap_or_default(),
            args: obj.string_array("args"),
            token_context: obj.string("token_context"),
            value: obj
                .string("value")
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
        })
    }
}

/// Declare your contract's entry point.
///
/// Give it a function taking a [`Call`] and returning a [`Reply`].
#[macro_export]
macro_rules! entrypoint {
    ($handler:path) => {
        /// Entry point called by the Retium runtime.
        ///
        /// # Safety
        /// The runtime passes a readable input region and a writable output
        /// region inside this module's own memory.
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub unsafe extern "C" fn on_use(input_ptr: i32, input_len: i32, output_ptr: i32) -> i32 {
            $crate::__dispatch($handler, input_ptr, input_len, output_ptr)
        }
    };
}

/// Machinery behind [`entrypoint!`]. Not part of the stable surface.
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub fn __dispatch(
    handler: fn(Call) -> Reply,
    input_ptr: i32,
    input_len: i32,
    output_ptr: i32,
) -> i32 {
    let input = match String::from_utf8(read_input(input_ptr, input_len)) {
        Ok(s) => s,
        Err(_) => {
            return write_reply(
                output_ptr,
                "{\"ok\":false,\"error\":\"input was not UTF-8\"}",
            )
        }
    };
    let reply = match Call::from_json(&input) {
        Ok(call) => handler(call),
        Err(e) => Err(e),
    };
    let body = match reply {
        Ok(body) => body,
        Err(message) => format!(
            "{{\"ok\":false,\"error\":{}}}",
            events::json_string(&message)
        ),
    };
    write_reply(output_ptr, &body)
}

/// Copy the call input out of the module's memory.
///
/// The runtime writes the input at offset 0, so `input_ptr` is 0. That is an
/// ordinary wasm address, but Rust keeps address 0 to mean "no reference": a
/// `&u8` pointing there reads as `None`. The bytes are therefore read through
/// the raw pointer and copied, and no reference to them is ever formed.
#[cfg(target_arch = "wasm32")]
fn read_input(input_ptr: i32, input_len: i32) -> Vec<u8> {
    let base = input_ptr as u32 as usize;
    let len = input_len.max(0) as usize;
    let mut input = Vec::with_capacity(len);
    for offset in 0..len {
        input.push(unsafe { core::ptr::read_volatile((base + offset) as *const u8) });
    }
    input
}

#[cfg(target_arch = "wasm32")]
fn write_reply(output_ptr: i32, body: &str) -> i32 {
    let bytes = body.as_bytes();
    if bytes.len() > MAX_OUTPUT {
        let short = b"{\"ok\":false,\"error\":\"reply too large\"}";
        unsafe {
            core::ptr::copy_nonoverlapping(short.as_ptr(), output_ptr as *mut u8, short.len());
        }
        return short.len() as i32;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), output_ptr as *mut u8, bytes.len());
    }
    bytes.len() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_read_from_the_node_json() {
        let input = r#"{"action":"transfer","caller":"rete-alice","args":["rete-bob","10"],"token_context":null,"value":"250"}"#;
        let call = Call::from_json(input).unwrap();
        assert_eq!(call.action(), "transfer");
        assert_eq!(call.caller(), "rete-alice");
        assert_eq!(call.args(), ["rete-bob", "10"]);
        assert_eq!(call.arg(0).unwrap(), "rete-bob");
        assert_eq!(call.arg_u128(1).unwrap(), 10);
        assert_eq!(call.value(), 250);
        assert_eq!(call.token_context(), None);
    }

    #[test]
    fn the_caller_cannot_be_forged_through_an_argument() {
        // An attacker puts something that looks like a caller field inside an
        // argument. A real parse keeps them apart; a substring search would not.
        let input = r#"{"action":"admin","caller":"rete-attacker","args":["\"caller\":\"rete-admin\""],"token_context":null,"value":"0"}"#;
        let call = Call::from_json(input).unwrap();
        assert_eq!(
            call.caller(),
            "rete-attacker",
            "caller must come from the node's field, never from an argument"
        );
    }

    #[test]
    fn field_order_does_not_change_the_answer() {
        let a =
            Call::from_json(r#"{"caller":"x","value":"7","action":"go","args":["1"]}"#).unwrap();
        assert_eq!((a.caller(), a.action(), a.value()), ("x", "go", 7));
    }

    #[test]
    fn a_missing_argument_says_which_one() {
        let call =
            Call::from_json(r#"{"action":"mint","caller":"a","args":[],"value":"0"}"#).unwrap();
        let err = call.arg(0).unwrap_err();
        assert!(err.contains("argument 1"), "{err}");
        assert!(err.contains("mint"), "{err}");
    }

    #[test]
    fn a_non_numeric_amount_is_refused() {
        let call = Call::from_json(r#"{"action":"mint","caller":"a","args":["ten"],"value":"0"}"#)
            .unwrap();
        assert!(call.arg_u128(0).is_err());
    }

    #[test]
    fn replies_are_valid_json_even_with_quotes_inside() {
        assert_eq!(ok("all good").unwrap(), r#"{"ok":true,"data":"all good"}"#);
        assert_eq!(
            ok("say \"hi\"").unwrap(),
            r#"{"ok":true,"data":"say \"hi\""}"#
        );
    }
}
