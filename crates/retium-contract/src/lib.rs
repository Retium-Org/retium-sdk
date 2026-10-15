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
//! 2. **A refusal undoes the call.** When a user calls you and you return an
//!    error, the chain discards every write, event and transfer of that call
//!    and keeps your message as the reason. When another contract calls you, it
//!    gets your error as an ordinary reply, and if it carries on your writes
//!    stay. In a contract other contracts call, check first and write second,
//!    or call [`fail`], which always undoes your writes.
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
/// Returning an `Err` from a handler produces a JSON error reply. When the
/// call came from a user, the chain treats that reply as a refusal and undoes
/// the whole call, and any RTM attached goes back to the sender. When another
/// contract called you, it gets the reply as data and your writes stay unless
/// that contract fails too.
pub type Error = String;

/// The result of a handler.
pub type Reply = Result<String, Error>;

/// A successful reply carrying `data`.
///
/// The chain requires a reply to be valid JSON, so this wraps `data` as
/// `{"ok":true,"data":"..."}` with the value escaped for you.
pub fn ok(data: &str) -> Reply {
    Ok(format!(
        "{{\"ok\":true,\"data\":{}}}",
        events::json_string(data)
    ))
}

/// A failed reply carrying `message`.
///
/// On a call from a user, the chain undoes everything the call did and keeps
/// the message as the reason. The sender still pays gas. On a call from
/// another contract, the caller receives this reply and decides, so check
/// before you write if other contracts call you.
pub fn error(message: &str) -> Reply {
    Err(message.to_string())
}

/// Abandon the call and undo everything it has done.
///
/// This traps the module. The chain discards every write, event and transfer
/// the call made, including those of any contract it called. A contract that
/// called you gets an `Err` back instead of a reply. The sender still pays gas
/// and uses their nonce.
///
/// A trap carries no message, so `reason` is not seen on chain. It is shown
/// when a handler is run off-chain in tests. To give a user a reason, return
/// [`error`] instead.
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

    /// Who signed this call. The chain proves it, so you do not have to.
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
        let value = match obj.string("value")? {
            None => 0,
            Some(raw) => raw
                .parse()
                .map_err(|_| format!("'value' must be a whole number, got '{raw}'"))?,
        };
        Ok(Self {
            action: obj.string("action")?.unwrap_or_default(),
            caller: obj.string("caller")?.unwrap_or_default(),
            args: obj.strings("args")?,
            // Reserved. An unknown shape reads as none and the call still runs.
            token_context: obj.string("token_context").unwrap_or(None),
            value,
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

/// Embed your contract's RCP-1 metadata in the wasm.
///
/// The chain reads it at deploy and shows it to wallets and explorers. Give
/// all seven fields as JSON. The chain refuses the deploy if one is missing.
///
/// ```ignore
/// retium_contract::rcp_metadata!(r#"{"rcp_type":"counter","traits":["demo"],
///     "capabilities":["execute"],"lockable_fields":[],"evolves_from":"",
///     "weight":5,"version":"v1.0.0"}"#);
/// ```
///
/// `weight` must fit the chain's analysis of your code. For a Rust contract
/// that is 5. `retium deploy --dry-run` checks all of it before you pay gas.
#[macro_export]
macro_rules! rcp_metadata {
    ($json:literal) => {
        #[cfg(target_arch = "wasm32")]
        const __RCP_METADATA_TEXT: &str = concat!("RCP_METADATA:", $json, "\0");
        /// RCP-1 metadata, read by the chain at deploy.
        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub static RCP_METADATA: [u8; __RCP_METADATA_TEXT.len()] =
            $crate::__text_bytes(__RCP_METADATA_TEXT);
    };
}

/// Machinery behind [`rcp_metadata!`]. Not part of the stable surface.
#[doc(hidden)]
pub const fn __text_bytes<const N: usize>(text: &str) -> [u8; N] {
    let bytes = text.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = bytes[i];
        i += 1;
    }
    out
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
/// ordinary wasm address, but Rust keeps address 0 to mean "no reference", so a
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
        // argument. A real parse keeps them apart and a substring search would not.
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
    fn a_call_with_a_field_of_the_wrong_kind_is_refused() {
        for bad in [
            r#"{"action":"go","caller":42,"args":[],"value":"0"}"#,
            r#"{"action":["go"],"caller":"a","args":[],"value":"0"}"#,
            r#"{"action":"go","caller":"a","args":["1",2,"3"],"value":"0"}"#,
            r#"{"action":"go","caller":"a","args":"1","value":"0"}"#,
            r#"{"action":"go","caller":"a","args":[],"value":5}"#,
            r#"{"action":"go","caller":"a","args":[],"value":"five"}"#,
            r#"{"action":"go","caller":"a","args":[],"value":"-1"}"#,
            r#"{"action":"go","caller":"a","args":[],"value":"0"} trailing"#,
        ] {
            assert!(Call::from_json(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn fields_the_crate_does_not_know_do_not_break_a_call() {
        let input = r#"{"action":"go","caller":"a","args":["1"],"token_context":{"id":"t"},"value":"3","later":[1,{"x":"]"}]}"#;
        let call = Call::from_json(input).unwrap();
        assert_eq!((call.action(), call.caller(), call.value()), ("go", "a", 3));
        assert_eq!(call.args(), ["1"]);
        assert_eq!(call.token_context(), None);
        // A call with no value field at all.
        let call = Call::from_json(r#"{"action":"go","caller":"a","args":[]}"#).unwrap();
        assert_eq!(call.value(), 0);
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
