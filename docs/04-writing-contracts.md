# Writing contracts

Retium's contract model is small. This page is the whole of it.

## The shape of a contract

One entry point. The chain hands you a call and reads back a reply.

```rust
use retium_contract::{entrypoint, error, ok, storage, Call, Reply};

entrypoint!(handle);

pub fn handle(call: Call) -> Reply {
    match call.action() {
        "set" => {
            storage::set(call.arg(0)?, call.arg(1)?)?;
            ok("stored")
        }
        "get" => ok(&storage::get(call.arg(0)?)?.unwrap_or_default()),
        other => error(&format!("unknown action: {other}")),
    }
}
```

A call gives you:

| | |
|---|---|
| `call.action()` | the method name |
| `call.caller()` | who signed it — **proven by the chain** |
| `call.args()` | the arguments, in order |
| `call.value()` | RTM attached to this call, in atomic units |

`caller` is the foundation of every permission check you will write. You never verify a signature yourself; the chain already did.

## Rule 1: nothing in memory survives

Your module is rebuilt for every single call. Globals, `static`, lazily-initialised caches — all wiped. This is the mistake that costs new Retium developers the most time:

```rust
static mut COUNT: u64 = 0;           // wrong: always 0 at the start of a call
storage::set_u128("count", 5)?;      // right: state the chain keeps
```

Storage is a key-value store that belongs to your contract. Nobody else can write it.

```rust
storage::set("owner", call.caller())?;
let owner = storage::get_or("owner", "")?;
let total = storage::add_u128("supply", 100)?;   // refuses to overflow
storage::sub_u128("balance:alice", 10)?;         // refuses to go below zero
storage::remove("temp")?;
```

Values are up to 64 KiB. Storage is permanent and costs fuel by size — store what you need, not everything you have.

## Rule 2: check first, write second

Returning an error is **not** a rollback. If you wrote state and then returned an error, the writes stay:

```rust
// wrong
storage::sub_u128("balance:alice", amount)?;
if amount > limit {
    return error("over the limit");     // alice's balance is already reduced
}
```

Two correct options:

```rust
// check first
if amount > limit {
    return error("over the limit");
}
storage::sub_u128("balance:alice", amount)?;
```

```rust
// or undo everything
retium_contract::fail("over the limit");   // traps: every write, event and
                                            // transfer in this call is discarded
```

`fail` is the only way to be certain a half-finished change does not stick. A trap still costs the sender their gas and their nonce; it just leaves no state behind.

## Rule 3: be deterministic

Every keeper runs your contract and must reach the same answer. There is no clock, no randomness and no network available. Use `env::block_height()` or `env::block_time_ms()` when you need a notion of time — those are agreed by the chain. Avoid floating point.

## Rule 4: money moves in one direction

A contract can hold RTM and send its own:

```rust
use retium_contract::call;

let mine = call::rtm_balance_of(&env::contract_id()?)?;
call::send_rtm(call.caller(), payout)?;
```

A contract **cannot** take RTM from the caller's wallet. There is no allowance mechanism to abuse and no "approve" to forget. The only money you receive from a caller is what they attached to the call:

```rust
if call.value() < price {
    return error("not enough RTM attached");
}
```

The caller signed that amount, so they always knew what they were paying.

## Rule 5: respect the limits

| Limit | Value |
|---|---|
| Reply | 4 KiB |
| Stored value | 64 KiB per key |
| Event payload | 4 KiB, type name 64 bytes |
| Contract size | 1 MiB |
| Call depth | 64 |
| Fuel | one budget per transaction |

Fuel is the one to respect. Storage writes cost by size, cross-contract calls cost a lot, and signature checks cost a great deal. Running out fails the call after the sender has paid. Never loop over user-supplied input without a bound.

## Events

Events are how the outside world learns what happened. Your contract cannot read them back, so never use them as storage.

```rust
use retium_contract::events;

events::emit_fields("transfer", &[
    ("from", call.caller()),
    ("to", to),
    ("amount", &amount.to_string()),
])?;
```

Always build payloads with `emit_fields` or `events::json_string`. A quote or backslash in a user-supplied name will otherwise produce broken JSON that no indexer can read — and, worse, a name chosen carefully could inject fields an indexer trusts.

## Calling other contracts

```rust
let answer = call::contract(token_id, "balance_of", &[user])?;
call::contract_with_value(pool_id, "deposit", &[], 1_000_000)?;
```

If the other contract traps or runs out of fuel, its writes are undone and you get an `Err`. Your own writes are not undone unless you `fail` too.

A contract that answers with an error reply has not failed in that sense. It ran to the end, its writes stay, and you get its reply as `Ok`. Check it when the call must have worked:

```rust
let reply = call::contract(token_id, "transfer", &[to, &amount.to_string()])?;
if call::is_error_reply(&reply) {
    retium_contract::fail("token transfer was refused");
}
```

A contract cannot call itself, directly or in a circle.

## Testing before you deploy

Handlers are ordinary Rust. Off-chain, storage reads return "not set", so you can test every decision a handler makes without a node:

```rust
#[test]
fn only_the_owner_may_reset() {
    let call = Call::from_json(
        r#"{"action":"reset","caller":"rete-mallory","args":[],"value":"0"}"#
    ).unwrap();
    assert!(handle(call).is_err());
}
```

`cargo test -p your_contract` runs those on your machine. Then build for the chain:

```bash
cargo build --release --target wasm32-unknown-unknown -p your_contract
```

## A worked example

[`examples/hello_counter`](../examples/hello_counter) is a complete contract with an owner, a guarded action, events and tests. It is under 120 lines. Read it before writing your own.

## Checklist before you deploy

- [ ] No `static mut` or global state — everything in `storage`
- [ ] Every write happens after its checks, or the failure path calls `fail`
- [ ] Owner recorded in `init`, not hardcoded
- [ ] Arithmetic uses the checked helpers (`add_u128`, `sub_u128`) or your own checks
- [ ] Loops are bounded
- [ ] Events built with `emit_fields` / `json_string`
- [ ] RCP-1 metadata declared with an honest weight
- [ ] Tests for the refusal paths, not just the happy path
- [ ] Built with `--release` and deployed from a dry run you checked

Next: [RCP-1](03-rcp-1.md) · [Transactions](05-transactions.md) · [Verifying code](06-verifying-code.md)
