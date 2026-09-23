# retium-contract

Write [Retium](https://retium.org) smart contracts in Rust. Storage, events, cross-contract calls and the `on_use` entry point, as safe wrappers over the chain's host functions.

```rust
use retium_contract::{entrypoint, error, ok, storage, Call, Reply};

entrypoint!(handle);

pub fn handle(call: Call) -> Reply {
    match call.action() {
        "set" => { storage::set(call.arg(0)?, call.arg(1)?)?; ok("stored") }
        "get" => ok(&storage::get(call.arg(0)?)?.unwrap_or_default()),
        other => error(&alloc::format!("unknown action: {other}")),
    }
}
```

Build with `--target wasm32-unknown-unknown`. Off-chain the host functions are stubbed, so handlers can be unit-tested with plain `cargo test`.

Read [Writing contracts](https://github.com/Retium-Org/retium-sdk/blob/main/docs/04-writing-contracts.md) before your first deploy — the model has a few rules that decide whether a contract works at all. Licence: Apache-2.0.
