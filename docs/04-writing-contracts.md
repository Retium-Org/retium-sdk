# Writing contracts

Retium's contract model is small, and this page covers all of it.

## The shape of a contract

There is one entry point. The chain hands you a call and reads back a reply.

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]
extern crate alloc;

use alloc::format;
use retium_contract::{entrypoint, error, ok, storage, Call, Reply};

entrypoint!(handle);

retium_contract::rcp_metadata!(
    r#"{"rcp_type":"notes","traits":["demo"],"capabilities":["execute"],"lockable_fields":[],"evolves_from":"","weight":5,"version":"v1.0.0"}"#
);

pub fn handle(call: Call) -> Reply {
    match call.action() {
        "set" => {
            storage::set(call.arg(0)?, call.arg(1)?)?;
            ok("stored")
        }
        "get" => ok(&storage::get(call.arg(0)?)?.unwrap_or_default()),
        other => error(&format!("unknown action {other}")),
    }
}
```

A contract is a library crate built for `wasm32-unknown-unknown`. On wasm it has no standard library, because `retium-contract` brings its own allocator and panic handler, so keep the first lines above as they are. Its `Cargo.toml` looks like this.

```toml
[package]
name = "notes"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
retium-contract = { git = "https://github.com/Retium-Org/retium-sdk" }

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

`cdylib` produces the wasm module, and `rlib` lets `cargo test` run your handlers on your own machine. Build with `cargo build --release --target wasm32-unknown-unknown`.

A call gives you four things.

| | |
|---|---|
| `call.action()` | the method name |
| `call.caller()` | who signed it, **proven by the chain** |
| `call.args()` | the arguments, in order, as strings |
| `call.value()` | RTM attached to this call, in atomic units |

`caller` is the base of every permission check you will write. You never verify a signature yourself, because the chain already did.

`rcp_metadata!` embeds the record wallets and explorers read. All seven fields are required, and the chain refuses the deploy if one is missing. See [RCP-1](03-rcp-1.md).

## Arguments

An argument from a user may not contain `,` `(` `)` or `::`, and a method name is letters, digits and `_`. The SDK checks this before signing.

Need to pass structured data? Use several plain arguments, or encode it (hex, for example) and decode it in the contract.

Calls from other contracts are not limited this way. They carry arguments as a JSON array, so any text arrives intact.

## Rule 1. Nothing in memory survives

Your module is rebuilt for every call. Globals, `static` values and caches are all wiped. This is the mistake that costs new Retium developers the most time.

```rust
static mut COUNT: u64 = 0;           // wrong, always 0 at the start of a call
storage::set_u128("count", 5)?;      // right, state the chain keeps
```

Storage is a key-value store that belongs to your contract. Nobody else can write it.

```rust
storage::set("owner", call.caller())?;
let owner = storage::get_or("owner", "")?;
let total = storage::add_u128("supply", 100)?;   // refuses to overflow
storage::sub_u128("balance:alice", 10)?;         // refuses to go below zero
storage::remove("temp")?;
```

Keys are up to 1 KiB and values up to 64 KiB. Storage is permanent and costs fuel by size, so store what you need, not everything you have.

## Rule 2. A refusal undoes the call

When a user calls your contract and you return an error, the chain undoes the whole call. Every write, event and transfer is discarded, any RTM the user attached goes back to them, and your message is kept as the reason. The user still pays gas.

```rust
if amount > limit {
    return error("over the limit");
}
```

`retium_contract::fail` undoes the call the same way by trapping the module, but a trap carries no message, so the user sees no reason. Use `error` when the reason matters.

This changes when another contract calls you. The caller receives your error as an ordinary reply and decides what to do with it. If it carries on, your writes stay. So in a contract that other contracts call, check before you write, or use `fail`, which always undoes your writes and gives the caller an `Err`.

## Rule 3. Be deterministic

Every keeper runs your contract and must reach the same answer. There is no clock, no randomness and no network. Use `env::block_height()` or `env::block_time_ms()` when you need a notion of time, because those are agreed by the chain. Avoid floating point.

## Rule 4. Money moves in one direction

A contract can hold RTM and send its own.

```rust
use retium_contract::{call, env};

let mine = call::rtm_balance_of(&env::contract_id()?)?;
call::send_rtm(call.caller(), payout)?;
```

A contract **cannot** take RTM from the caller's wallet. There is no allowance to abuse and no approval to forget. The only money you receive from a caller is what they attached to the call, which they signed.

```rust
if call.value() < price {
    return error("not enough RTM attached");
}
```

## Rule 5. Respect the limits

| Limit | Value |
|---|---|
| Contract size | 1 MiB |
| Reply | 4 KiB |
| Storage key | 1 KiB |
| Storage value | 64 KiB |
| Event | 4 KiB of data, 64 bytes of type name |
| Memory | 32 MiB |
| Call depth | 64 |
| Fuel | 100,000,000 per transaction |
| Time | 5 seconds per transaction, 2 seconds per free read |

Fuel is the one to watch. Storage writes cost by size, a cross-contract call costs 50,000 before the other contract does anything, and a signature check costs 80,000. Running out fails the call after the sender has paid. Never loop over user-supplied input without a bound.

## Events

Events tell the outside world what happened. Your contract cannot read them back, so never use them as storage.

```rust
use retium_contract::events;

events::emit_fields("transfer", &[
    ("from", call.caller()),
    ("to", to),
    ("amount", &amount.to_string()),
])?;
```

Always build payloads with `emit_fields` or `events::json_string`. A quote or backslash in a user-supplied name would otherwise produce broken JSON that no indexer can read, and a name chosen with care could add fields an indexer trusts.

## Calling other contracts

```rust
let answer = call::contract(token_id, "balance_of", &[user])?;
call::contract_with_value(pool_id, "deposit", &[], 1_000_000)?;
```

If the other contract traps, runs out of fuel or is refused by the chain, its writes are undone and you get an `Err`. Your own writes are not undone unless you fail too.

A contract that answers with an error reply has not failed in that sense. It ran to the end, its writes stay, and you get its reply as `Ok`. Check it whenever the call must have worked.

```rust
let reply = call::contract(token_id, "transfer", &[to, &amount.to_string()])?;
if call::is_error_reply(&reply) {
    retium_contract::fail("token transfer was refused");
}
```

A contract cannot call itself, directly or in a circle.

## Testing before you deploy

Handlers are ordinary Rust. Off-chain, storage reads return "not set", so you can test every decision a handler makes without a node.

```rust
#[test]
fn only_the_owner_may_reset() {
    let call = Call::from_json(
        r#"{"action":"reset","caller":"rete-mallory","args":[],"value":"0"}"#
    ).unwrap();
    assert!(handle(call).is_err());
}
```

`cargo test -p your_contract` runs those on your machine. Then build for the chain and check the build.

```bash
cargo build --release --target wasm32-unknown-unknown -p your_contract
retium deploy target/wasm32-unknown-unknown/release/your_contract.wasm --dry-run
```

## A worked example

[`examples/hello_counter`](../examples/hello_counter) is a complete contract with an owner, a guarded action, events, metadata and tests. Read it before writing your own.

## Other languages

The chain runs WebAssembly and does not know what produced it. The same counter is in [`examples/`](../examples) in AssemblyScript, C (also valid C++), Zig, Go and hand-written WebAssembly text, each with its build command, and one script, `examples/run-as-runtime.mjs`, checks any of them the way the chain runs them. Without `retium-contract` you talk to the chain through the interface in the [wire reference](08-wire-reference.md#6-contract-interface). Two rules there have no Rust equivalent, because Rust gets them right by default: keep your memory above 64 KiB, and declare the weight the chain scores your code at.

## Checklist before you deploy

- [ ] No `static mut` or global state. Everything lives in `storage`.
- [ ] Refusals return an error or call `fail`, and anything other contracts call checks before it writes.
- [ ] The owner is recorded in `init`, not hardcoded, and `init` is sent straight after the deploy.
- [ ] Arithmetic uses the checked helpers (`add_u128`, `sub_u128`) or your own checks.
- [ ] Loops are bounded.
- [ ] Events are built with `emit_fields` or `json_string`.
- [ ] `rcp_metadata!` is present with all seven fields and weight 5 (in another language, the weight the dry run reports).
- [ ] Tests cover the refusal paths, not just the happy path.
- [ ] Built with `--release`, and the dry run passed.

Read on with [RCP-1](03-rcp-1.md), [Tokens](09-tokens.md) or [Transactions](05-transactions.md).
