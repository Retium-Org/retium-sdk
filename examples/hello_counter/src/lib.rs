//! A counter, written the way a Retium contract should be written.
//!
//! It is deliberately small. Read it once and you have seen every part of the
//! model: one entry point, state in storage, checks before writes, events for
//! the outside world, and an owner that is recorded on first use rather than
//! hardcoded.
//!
//! Actions:
//!
//! | action | arguments | who may call it |
//! |---|---|---|
//! | `init` | — | the first caller, once |
//! | `bump` | optional step | anyone |
//! | `value` | — | anyone |
//! | `reset` | — | the owner |
//!
//! Build it:
//!
//! ```text
//! cargo build --release --target wasm32-unknown-unknown -p hello_counter
//! ```

#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

use alloc::format;
use alloc::string::ToString;

use retium_contract::{entrypoint, error, events, ok, storage, Call, Reply};

entrypoint!(handle);

/// Storage keys. Keep them in one place: they are your schema.
const OWNER: &str = "owner";
const COUNT: &str = "count";

/// The largest step a single call may add, so one call cannot make the counter
/// meaningless.
const MAX_STEP: u128 = 1_000;

pub fn handle(call: Call) -> Reply {
    match call.action() {
        "init" => init(&call),
        "bump" => bump(&call),
        "value" => ok(&storage::get_u128(COUNT)?.to_string()),
        "owner" => ok(&storage::get_or(OWNER, "")?),
        "reset" => reset(&call),
        other => error(&format!(
            "unknown action '{other}' — try init, bump, value, owner or reset"
        )),
    }
}

/// Record the first caller as the owner. Doing this in `init` rather than in a
/// constant means the code can be published and deployed by anyone without
/// handing control to whoever wrote it.
pub fn init(call: &Call) -> Reply {
    if !storage::get_or(OWNER, "")?.is_empty() {
        return error("already initialised");
    }
    storage::set(OWNER, call.caller())?;
    storage::set_u128(COUNT, 0)?;
    events::emit_fields("counter_created", &[("owner", call.caller())])?;
    ok(call.caller())
}

/// Add to the counter. The step is checked *before* anything is written, so a
/// refused call leaves no trace.
pub fn bump(call: &Call) -> Reply {
    let step = match call.args().first() {
        Some(_) => call.arg_u128(0)?,
        None => 1,
    };
    if step == 0 || step > MAX_STEP {
        return error(&format!("step must be between 1 and {MAX_STEP}"));
    }

    let total = storage::add_u128(COUNT, step)?;
    events::emit_fields(
        "counter_bumped",
        &[
            ("by", call.caller()),
            ("step", &step.to_string()),
            ("total", &total.to_string()),
        ],
    )?;
    ok(&total.to_string())
}

/// Only the owner may reset. `caller` is proven by the chain, so comparing it
/// is a complete check — no signature handling of your own is needed.
pub fn reset(call: &Call) -> Reply {
    let owner = storage::get_or(OWNER, "")?;
    if owner.is_empty() {
        return error("not initialised yet");
    }
    if call.caller() != owner {
        return error("only the owner may reset this counter");
    }
    storage::set_u128(COUNT, 0)?;
    events::emit_fields("counter_reset", &[("by", call.caller())])?;
    ok("0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(action: &str, caller: &str, args: &[&str]) -> Call {
        let args = args
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<alloc::vec::Vec<_>>()
            .join(",");
        Call::from_json(&format!(
            r#"{{"action":"{action}","caller":"{caller}","args":[{args}],"value":"0"}}"#
        ))
        .unwrap()
    }

    // Off-chain, storage reads always return "not set", so these tests cover the
    // decisions a handler makes rather than the chain's storage.

    #[test]
    fn an_unknown_action_is_reported_with_the_options() {
        let err = handle(call("fly", "rete-alice", &[])).unwrap_err();
        assert!(err.contains("unknown action 'fly'"), "{err}");
        assert!(err.contains("bump"), "{err}");
    }

    #[test]
    fn a_step_outside_the_allowed_range_is_refused() {
        assert!(handle(call("bump", "rete-alice", &["0"])).is_err());
        assert!(handle(call("bump", "rete-alice", &["1001"])).is_err());
        assert!(
            handle(call("bump", "rete-alice", &["ten"])).is_err(),
            "not a number"
        );
    }

    #[test]
    fn reset_is_refused_when_there_is_no_owner_yet() {
        let err = handle(call("reset", "rete-alice", &[])).unwrap_err();
        assert!(err.contains("not initialised"), "{err}");
    }

    #[test]
    fn value_reads_as_zero_before_anything_is_stored() {
        let reply = handle(call("value", "rete-alice", &[])).unwrap();
        assert!(reply.contains("\"data\":\"0\""), "{reply}");
    }
}
