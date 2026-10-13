# Retium SDK

Build on [Retium](https://retium.org). Create accounts, send transactions and write smart contracts. The SDK is Rust. Contracts are WebAssembly and can be written in Rust, AssemblyScript, C, C++, Zig, Go or anything else that produces a wasm module, and `examples/` has the same contract in each.

Retium signs with **ML-DSA-44** (FIPS 204), a post-quantum scheme. Keys and signatures are bigger than the elliptic-curve ones you may be used to (a public key is 1,312 bytes, a signature 2,421), but the work has the same shape. Build a payload, hash it, sign it, send it.

```rust
use retium_sdk::{key::SecretKey, rpc::Client, tx};

let key = SecretKey::read_key_file("my-wallet.sk")?;
let client = Client::new(retium_sdk::rpc::PUBLIC_RPC)?;

let wallet = client.wallet(&key.address()).await?.expect("funded account");
println!("balance {} RTM", wallet.balance_rtm);

let sent = tx::transfer(&client, &key, "rete...", "1.5").await?;
client.wait_for_final(&sent.tx_id, std::time::Duration::from_secs(120)).await?;
```

Building with an AI agent? Point it at [AGENTS.md](AGENTS.md). It explains every common task with exact commands, and [docs/12-ai-agents.md](docs/12-ai-agents.md) tells you what to expect.

## What is in here

| Crate / folder | What it is |
|---|---|
| [`crates/retium-sdk`](crates/retium-sdk) | The client with keys, addresses, amounts, signing, RPC and the `retium` command-line tool |
| [`crates/retium-contract`](crates/retium-contract) | What you write contracts against, with storage, events, cross-contract calls and the entry point |
| [`examples/hello_counter`](examples/hello_counter) | One complete, working contract in Rust. Read it before writing your own |
| [`examples/hello_counter_as`](examples/hello_counter_as), [`_c`](examples/hello_counter_c), [`_zig`](examples/hello_counter_zig), [`_go`](examples/hello_counter_go), [`_wat`](examples/hello_counter_wat) | The same contract in AssemblyScript, C, Zig, Go and WebAssembly text, for building without Rust |
| [`docs/`](docs) | How Retium works and how to build on it |

There is no template collection here on purpose. Retium contracts are small and the model is unusual enough that copying a template you do not understand is the fastest way to lose money. Read [docs/04-writing-contracts.md](docs/04-writing-contracts.md) first. It is a short read.

## Install

```bash
git clone https://github.com/Retium-Org/retium-sdk
cd retium-sdk
cargo install --path crates/retium-sdk       # the `retium` command
rustup target add wasm32-unknown-unknown      # to build contracts
```

Rust 1.85 or newer. The repo pins a toolchain in `rust-toolchain.toml` so contract builds are reproducible.

## Getting started

The SDK comes with no account and no keys. You make your own, fund it, and get an API key. It takes about fifteen minutes.

### 1. Make an account

```bash
retium key new my-wallet.sk --show-seed
```

This writes `my-wallet.sk` and prints your address and a 64-character seed. Write the seed down and keep it offline. Anyone who has it owns the account, and without it a lost key file cannot be recovered.

Keep the key file outside any repository you push. This repo ignores `*.sk` files, but a copy elsewhere is up to you. Then point the tool at it.

```bash
export RETIUM_KEY=$PWD/my-wallet.sk
retium balance
```

Already have a Retium wallet? Copy its private key (64 hex characters) and turn it into a key file instead.

```bash
retium key import my-wallet.sk --seed <64 hex characters>
```

### 2. Add it to the Retium wallet

Open [wallet.retium.org](https://wallet.retium.org), choose Import key and paste the seed from step 1. The wallet and the SDK derive the same address from it. You need the wallet for steps 3 and 4.

### 3. Fund the account

Testnet coins have no value and are never sold.

- **RTD (Retium Dollar).** Claim it on [retium.money](https://retium.money) after linking your Discord account. You can pay gas in RTD by adding `--fee RTD` to any command that sends a transaction. It costs 1.5 times the RTM fee.
- **RTM.** Buy it with RTD on [retium.money](https://retium.money), or ask in the Retium Discord.

A deploy costs $0.50 of gas and a simple call $0.01 to $0.50, depending on the contract's weight, charged in RTM at the network's reference price or in RTD at 1.5 times that.

### 4. Get an API key

Reads are free and need no key. Anything that sends a transaction needs one.

Open [retium.net/api-keys](https://retium.net/api-keys), connect your wallet and create a key. The free tier allows 100,000 requests a day. The page shows the secret (it starts with `rk_`) once. Copy it then.

```bash
export RETIUM_API_KEY=rk_...
```

Pass it through the environment, never as a command-line argument. Once you have one key, `retium api-key new <label>`, `retium api-key list` and `retium api-key revoke <hash>` manage the rest from the command line.

### 5. Deploy the example

```bash
retium health                        # is the network up?
cargo build --release --target wasm32-unknown-unknown -p hello_counter
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm --dry-run
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm
retium tx <tx-id> --wait             # final, and did the contract accept it?
retium call <contract-id> init
retium call <contract-id> bump 5
retium query <contract-id> value     # free, no transaction
retium verify <contract-id> target/wasm32-unknown-unknown/release/hello_counter.wasm
```

`--dry-run` runs every check the chain runs on a deploy and shows the contract id before you spend anything. Send `init` straight after the deploy and read `owner` back, because until `init` has run anyone may call it.

The full walk-through is in [docs/02-quickstart.md](docs/02-quickstart.md).

## Documentation

1. [What Retium is](docs/01-what-is-retium.md). Accounts, ticks, finality and who runs what
2. [Quickstart](docs/02-quickstart.md). From account to deployed contract
3. [RCP-1](docs/03-rcp-1.md). The contract standard with identity, metadata, weight and traits
4. [Writing contracts](docs/04-writing-contracts.md). The entry point, storage, events, money and the rules that bite
5. [Transactions](docs/05-transactions.md). What a signature covers, nonces, gas and finality
6. [Verifying code](docs/06-verifying-code.md). Proving a deployed contract is the source you published
7. [Security](docs/07-security.md). Keys, and what the chain does and does not protect you from
8. [Wire reference](docs/08-wire-reference.md). The HTTP interface, the signed byte layout and the contract interface, for building in another language
9. [Tokens](docs/09-tokens.md). What wallets and other contracts expect from a fungible token
10. [Connecting a wallet](docs/10-connecting-a-wallet.md). Asking the Retium wallet to sign from a web page
11. [Errors](docs/11-errors.md). What the messages mean and what to do
12. [Building with an AI agent](docs/12-ai-agents.md). Handing the work to an assistant, and keeping your keys and money safe while it does it

## Testnet

The public test network exists for building and breaking things. **RTM and RTD on it have no value.** Nobody should pay for testnet coins, and nothing here can be used to make them look like they are worth something.

## Contributing and reporting problems

Issues and pull requests are welcome. For anything that looks like a security problem, please read [SECURITY.md](SECURITY.md) first and report it privately rather than in an issue.

## Licence

Apache-2.0. See [LICENSE](LICENSE).
