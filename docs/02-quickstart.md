# Quickstart

From nothing to a deployed contract. Expect about ten minutes, most of it waiting for `cargo`.

## 1. Install

```bash
git clone https://github.com/Retium-Org/retium-sdk
cd retium-sdk
cargo install --path crates/retium-sdk
rustup target add wasm32-unknown-unknown
```

Check the network is reachable:

```bash
retium health
# ok — tick 812, node 3.0.3
```

## 2. Make an account

```bash
retium key new my-wallet.sk --show-seed
```

```
new account
  address  rete4928105eac4b9a…f64e
  key file my-wallet.sk
  seed     8f3c…               ← write this down, offline

Write the seed down and keep it offline. It is the account.
```

The seed is shown once. The key file is the same secret in a file; back up one or both, keep them out of git, and never paste either into a chat, an issue or a support request.

Point the tool at your key so you can stop typing `--key`:

```bash
export RETIUM_KEY=my-wallet.sk       # Windows: setx RETIUM_KEY my-wallet.sk
retium balance
```

## 3. Fund the account

Every transaction costs gas, so the account needs RTM before it can do anything. How to get testnet RTM for an account is described on [retium.org](https://retium.org) — it has no value and exists only so you can pay gas while you build.

```bash
retium balance
# rete4928…
#   balance    25 RTM
#   staked     0 PIE
#   next nonce 0
```

## 4. Send some

```bash
retium send rete…someone… 1.5
# sent 1.5 RTM to rete…
#   tx tx_9f2c…

retium tx tx_9f2c… --wait
# tx_9f2c… is HardFinal
```

## 5. Build the example contract

```bash
cargo build --release --target wasm32-unknown-unknown -p hello_counter
```

Look before you leap — this prints the id the contract will have, without sending anything:

```bash
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm --dry-run
# would deploy … (36954 bytes)
#   contract id de6ea51a…
#   code hash   286d8403…
```

The id is derived from the code, your address and the metadata. Deploying the same bytes twice from the same account gives the same id and is refused the second time.

## 6. Deploy it

```bash
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm
# deployed …
#   contract id de6ea51a…
#   code hash   286d8403…
#   tx          tx_4411…
```

A deploy is weight 5, so it costs about $0.50 worth of RTM.

## 7. Use it

```bash
retium call de6ea51a… init          # first caller becomes the owner
retium call de6ea51a… bump 5        # add 5
retium query de6ea51a… value        # free read, no transaction
# {"ok":true,"data":"5"}
```

`call` writes and costs gas. `query` reads and costs nothing — use it for anything that does not change state.

## 8. Prove what is deployed

```bash
retium verify de6ea51a… target/wasm32-unknown-unknown/release/hello_counter.wasm
# match
#   de6ea51a…
#   runs exactly the code in target/…/hello_counter.wasm
```

This is how anyone checks that a contract is the source you published. See [Verifying code](06-verifying-code.md).

## From Rust instead of the shell

Everything above is library calls; the tool is a thin front end.

```rust
use retium_sdk::{key::SecretKey, rpc::Client, tx};

let key = SecretKey::read_key_file("my-wallet.sk")?;
let client = Client::new(retium_sdk::rpc::PUBLIC_RPC)?;

let wasm = std::fs::read("hello_counter.wasm")?;
let sent = tx::deploy(&client, &key, &wasm, None).await?;
client.wait_for_final(&sent.tx_id, std::time::Duration::from_secs(300)).await?;

let reply = tx::call(&client, &key, sent.contract_id.as_deref().unwrap(), "bump", &["5".into()], None).await?;
println!("bumped in {}", reply.tx_id);
```

## When something fails

| What you see | What it means |
|---|---|
| `the node is busy, try again shortly` | Send the same transaction again in a moment. |
| `the node refused this transaction` | Do not retry — fix it. Usually nonce, balance or signature. |
| `nonce` in the message | Something else spent from this account meanwhile; rebuild the transaction. |
| `HTTP 413` on deploy | The contract is over 1 MiB. Build with `--release`. |
| `contract is … bytes; the limit is …` | The same, caught locally before you upload. |

Next: [Writing contracts](04-writing-contracts.md).
