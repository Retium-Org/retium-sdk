# Retium SDK

Build on [Retium](https://retium.org) in Rust: create accounts, send transactions, and write smart contracts.

Retium signs with **ML-DSA-44** (FIPS 204), a post-quantum scheme. Keys and signatures are bigger than the elliptic-curve ones you may be used to — a public key is 1,312 bytes, a signature 2,421 — but the work is the same shape: build a payload, hash it, sign it, send it.

```rust
use retium_sdk::{key::SecretKey, rpc::Client, tx};

let key = SecretKey::read_key_file("my-wallet.sk")?;
let client = Client::new(retium_sdk::rpc::PUBLIC_RPC)?;

let wallet = client.wallet(&key.address()).await?.expect("funded account");
println!("balance {} RTM", wallet.balance_rtm);

let sent = tx::transfer(&client, &key, "rete…", "1.5").await?;
client.wait_for_final(&sent.tx_id, std::time::Duration::from_secs(120)).await?;
```

## What is in here

| Crate / folder | What it is |
|---|---|
| [`crates/retium-sdk`](crates/retium-sdk) | The client: keys, addresses, amounts, signing, RPC, and the `retium` command-line tool |
| [`crates/retium-contract`](crates/retium-contract) | What you write contracts against: storage, events, cross-contract calls, the entry point |
| [`examples/hello_counter`](examples/hello_counter) | One complete, working contract — read this before writing your own |
| [`docs/`](docs) | How Retium works and how to build on it |

There is no template collection here on purpose. Retium contracts are small and the model is unusual enough that copying a template you do not understand is the fastest way to lose money. Read [docs/04-writing-contracts.md](docs/04-writing-contracts.md); it is a short read.

## Install

```bash
git clone https://github.com/Retium-Org/retium-sdk
cd retium-sdk
cargo install --path crates/retium-sdk       # the `retium` command
rustup target add wasm32-unknown-unknown      # to build contracts
```

Rust 1.85 or newer. The repo pins a toolchain in `rust-toolchain.toml` so contract builds are reproducible.

## Five minutes

```bash
retium key new my-wallet.sk          # make an account
export RETIUM_KEY=my-wallet.sk
retium balance                       # 0, until you fund it
retium health                        # is the network up?
```

Fund the account (see [retium.org](https://retium.org)), then deploy the example:

```bash
cargo build --release --target wasm32-unknown-unknown -p hello_counter
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm --dry-run
retium deploy target/wasm32-unknown-unknown/release/hello_counter.wasm
retium call <contract-id> init
retium call <contract-id> bump 5
retium query <contract-id> value     # free, no transaction
```

Full walk-through: [docs/02-quickstart.md](docs/02-quickstart.md).

## Documentation

1. [What Retium is](docs/01-what-is-retium.md) — the model: accounts, ticks, finality, who runs what
2. [Quickstart](docs/02-quickstart.md) — account to deployed contract
3. [RCP-1](docs/03-rcp-1.md) — the contract standard: identity, metadata, weight, traits
4. [Writing contracts](docs/04-writing-contracts.md) — the entry point, storage, events, money, and the rules that bite
5. [Transactions](docs/05-transactions.md) — what a signature covers, nonces, gas, finality
6. [Verifying code](docs/06-verifying-code.md) — proving a deployed contract is the source you published
7. [Security](docs/07-security.md) — keys, what the chain does and does not protect you from
8. [Wire reference](docs/08-wire-reference.md) — the HTTP interface, the signed byte layout and the contract interface, for building in another language

## Testnet

The public test network exists for building and breaking things. **RTM on it has no value.** Nobody should pay for testnet RTM, and nothing here can be used to make it look like it is worth something.

## Contributing and reporting problems

Issues and pull requests are welcome. For anything that looks like a security problem, please read [SECURITY.md](SECURITY.md) first and report it privately rather than in an issue.

## Licence

Apache-2.0. See [LICENSE](LICENSE).
