# Quickstart

From nothing to a deployed contract on the public testnet. Expect about fifteen minutes, most of it waiting for `cargo`.

## 1. Install

```bash
git clone https://github.com/Retium-Org/retium-sdk
cd retium-sdk
cargo install --path crates/retium-sdk
rustup target add wasm32-unknown-unknown
```

Check the network is reachable.

```bash
retium health
# ok, tick 9, node ...
```

## 2. Make an account

The SDK ships with no account and no keys. Make your own.

```bash
retium key new my-wallet.sk --show-seed
```

```
new account
  address  rete4928105eac4b9a...f64e
  key file my-wallet.sk
  seed     8f3c...

Write the seed down and keep it offline. It is the account.
```

The seed is shown once. The key file holds the same secret. Back up one or both, keep them out of git, and never paste either into a chat, an issue or a support request.

Point the tool at your key so you can stop typing `--key`.

```bash
export RETIUM_KEY=$PWD/my-wallet.sk      # Windows: setx RETIUM_KEY %CD%\my-wallet.sk
retium balance
```

Already have a Retium wallet? Copy its private key (64 hex characters) and make a key file from it.

```bash
retium key import my-wallet.sk --seed <64 hex characters>
```

## 3. Add the account to the Retium wallet

Open [wallet.retium.org](https://wallet.retium.org), choose Import key and paste the seed. The wallet and the SDK derive the same address from it. You need the wallet to fund the account and to get an API key.

## 4. Fund the account

Every transaction costs gas, so the account needs coins before it can do anything. Testnet coins have no value and are never sold.

* **RTD.** Claim it on [retium.money](https://retium.money) after linking your Discord account. Add `--fee RTD` to any command that sends a transaction to pay its gas in RTD, at 1.5 times the RTM fee.
* **RTM.** Buy it with RTD in the Safehouse on [retium.money](https://retium.money), or ask in the Retium Discord.

```bash
retium balance
# rete4928...
#   balance    25 RTM
#              10 RTD
#   next nonce 0
```

## 5. Get an API key

Reads are free and need no key. Anything that sends a transaction needs one.

Open [retium.net/api-keys](https://retium.net/api-keys), connect your wallet and create a key. The free tier allows 100,000 requests a day and 5 a second. The secret starts with `rk_` and is shown once, so copy it then.

```bash
export RETIUM_API_KEY=rk_...
```

Pass it through the environment, not as a command-line argument. With one key in place, `retium api-key new`, `list`, `revoke` and `upgrade` manage the rest.

## 6. Send some

```bash
retium send rete...someone... 1.5
# sent 1.5 RTM to rete...
#   tx tx_9f2c...

retium tx tx_9f2c... --wait
# tx_9f2c... is HardFinal
```

## 7. Build the example contract

```bash
cargo build --release --target wasm32-unknown-unknown -p hello_counter
```

Check it before you spend anything. The dry run runs every check the chain runs on a deploy and prints the id the contract will have, without sending anything.

```bash
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm --dry-run
# would deploy ... (38793 bytes)
#   contract id c16837bf...
#   code hash   c439ee71...
#   type        counter
#   version     v1.0.0
#   weight      5 (the chain scores the code at 5)
#   traits      demo
#   metadata    embedded in the wasm
```

The id is derived from the code, your address and the deploy metadata. Deploying the same bytes twice from the same account gives the same id, and the second deploy is refused.

## 8. Deploy it

```bash
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm
# deployed ...
#   contract id c16837bf...
#   code hash   c439ee71...
#   tx          tx_4411...

retium tx tx_4411... --wait
# tx_4411... is HardFinal, result ok
```

A deploy is weight 5, so it costs $0.50 worth of RTM (or 0.75 RTD with `--fee RTD`).

## 9. Use it

```bash
retium call c16837bf... init          # the first caller becomes the owner
retium query c16837bf... owner        # must show your address
retium call c16837bf... bump 5        # add 5
retium query c16837bf... value        # free read, no transaction
# {"contract_id":"c16837bf...","method":"value","ok":true,"output":"{\"ok\":true,\"data\":\"5\"}","view":true}
```

`call` writes and costs gas. `query` reads and costs nothing. Use it for anything that does not change state.

Send `init` straight after the deploy. Until it has run once, anyone may call it, so the contract is yours only when `owner` shows your address. If it shows another, leave that contract and deploy again with a different `name` in a `--metadata` file, which gives a new contract id.

A contract can refuse a call. The call is still final and still costs gas, and `retium tx --wait` shows the reason and exits with an error.

```bash
retium call c16837bf... bump 5000
retium tx tx_aeb4... --wait
# error: tx_aeb4... is HardFinal, result failed
#   Contract failed on lead: contract refused the call: step must be between 1 and 1000
```

## 10. Prove what is deployed

```bash
retium verify c16837bf... target/wasm32-unknown-unknown/release/hello_counter.wasm
# match
#   c16837bf...
#   runs exactly the code in target/.../hello_counter.wasm
```

This is how anyone checks that a contract is the source you published. See [Verifying code](06-verifying-code.md).

## From Rust instead of the shell

Every command above is a library call, and the tool is a thin front end.

```rust
use retium_sdk::{key::SecretKey, rpc::{Client, Outcome}, tx};
use std::time::Duration;

let key = SecretKey::read_key_file("my-wallet.sk")?;
let client = Client::builder(retium_sdk::rpc::PUBLIC_RPC)
    .api_key(std::env::var("RETIUM_API_KEY")?)
    .build()?;

let wasm = std::fs::read("hello_counter.wasm")?;
let sent = tx::deploy(&client, &key, &wasm, None).await?;
client.wait_for_result(&sent.tx_id, Duration::from_secs(300)).await?;

let id = sent.contract_id.expect("a deploy answers with the contract id");
let bump = tx::call(&client, &key, &id, "bump", &["5".into()], None).await?;
let status = client.wait_for_result(&bump.tx_id, Duration::from_secs(300)).await?;
if let Outcome::Failed(reason) = status.outcome {
    eprintln!("refused: {reason}");
}
```

When something goes wrong, see [Errors](11-errors.md). To have an AI assistant do these steps for you, see [Building with an AI agent](12-ai-agents.md).

Read on with [Writing contracts](04-writing-contracts.md).
