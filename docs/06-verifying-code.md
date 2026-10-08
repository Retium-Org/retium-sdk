# Verifying code

How to prove that a contract running on Retium is the source someone published, and how to make your own contract easy to check.

## Why it works

When a contract is deployed, the chain records `blake3(wasm)` as its **code hash**. The chain computes it, so the deployer cannot declare it. Code never changes after deployment, so that hash is a permanent fingerprint of exactly what runs.

Everything else, such as name, symbol, traits, capabilities and version, is a claim by whoever deployed it. The code hash is the only part that is a fact.

## Checking a contract

```bash
git clone <the project's repository>
cd <project>
cargo build --release --target wasm32-unknown-unknown -p <contract>
retium verify <contract-id> target/wasm32-unknown-unknown/release/<contract>.wasm
```

```
match
  c16837bf...
  runs exactly the code in target/.../hello_counter.wasm
```

A mismatch prints both hashes and exits with an error. The deployed contract is then *not* the published source, and you should treat it as unknown code.

The same check from Rust looks like this.

```rust
let local = retium_sdk::contract::code_hash(&std::fs::read("contract.wasm")?);
let deployed = client.contract(contract_id).await?.and_then(|c| c.code_hash);
assert_eq!(Some(local), deployed);
```

Inside a contract, `env::code_hash_of(other_contract)` gives the same value, so a contract can refuse to talk to code it does not recognise.

## Making your contract verifiable

The same source must produce the same bytes on someone else's machine. Four things make that true.

1. **Pin the toolchain.** This repo's `rust-toolchain.toml` does it. Ship one with your project.
2. **Commit your `Cargo.lock`.** A different dependency version gives different bytes.
3. **Use one release profile** and keep it in the repository. This workspace sets the following.

   ```toml
   [profile.release]
   opt-level = "z"
   lto = true
   codegen-units = 1
   panic = "abort"
   strip = true
   ```

4. **Publish the hash and the exact build command** with your release, next to the contract id.

   ```
   contract  c16837bf...
   code hash c439ee71...
   built with rustc 1.91.1, cargo build --release --target wasm32-unknown-unknown -p hello_counter
   ```

If you post-process the wasm (with `wasm-opt`, for example), pin that tool's version too and say so. Otherwise nobody can reproduce your bytes.

## What verification does and does not tell you

**It tells you** that the code on chain is this source.

**It does not tell you** that the source is safe. Read what it does with owners, mints and funds. A verified contract can still let its owner mint without limit. Verification proves honesty about the code, not good intentions.

**It also does not tell you** that a contract with a familiar name is the real one. Anyone can deploy a contract calling itself anything. Start from the id published by the project you trust, then verify.

## A habit worth having

When you release a contract, publish three lines, the contract id, the code hash and the build command. It takes a minute, it makes your project checkable by anyone, and it is the difference between "trust us" and "check for yourself".

Read on with [Security](07-security.md).
