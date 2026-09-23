# What Retium is

Enough of the model to build on it confidently. You do not need to run a node, and you do not need to understand consensus to ship something.

## Accounts

An account is a key pair. The address is derived from the public key and carries a checksum:

```
rete ‖ hex(blake3(public key)) ‖ hex(checksum)     72 characters
```

A mistyped or truncated address fails the checksum, so it is refused rather than paid to. Validate anything a user typed:

```rust
retium_sdk::address::is_valid(&input)
```

Signatures are ML-DSA-44 (FIPS 204). A key file is 37 bytes: a header, a version byte and a 32-byte seed. **The seed is the account** — back it up and everything else can be rebuilt.

`rete_treasury` and `rete_burn` look like addresses but are not: they are ledger accounts the chain uses for gas and burns, and they can never be a sender.

## Money

RTM is the native token. One RTM is 10^18 atomic units. Everything signed and sent is atomic units as a decimal string, never a float:

```rust
retium_sdk::amount::parse_rtm("1.5")?;   // 1500000000000000000
retium_sdk::amount::format_rtm(1)?;      // "0.000000000000000001"
```

PIE is what RTM becomes when it is staked. It is not a tradable token you need to think about unless you run a validator.

## Who runs the network

Four kinds of node, all run by node operators, none of which you need:

| Role | What it does |
|---|---|
| **Keepers** | Execute transactions, hold the full ledger, agree on blocks |
| **Workers** | Check transactions independently before they become final |
| **Suits** | Confirm that keepers agreed, and finish a block's finality |
| **Readers** | Serve the public RPC — this is what your code talks to |

You reach the network through one address, `https://rpc.retium.org`. You never address keepers directly, and nothing you build needs a node of your own.

## Blocks, ticks and finality

Transactions go into blocks; blocks belong to ticks. A tick is a round of work, and later ticks carry more blocks than earlier ones by design — a slow tick is not a stuck chain.

A transaction moves through states:

| State | Meaning |
|---|---|
| `Pending` | the network has it |
| `SoftFinal` | it has executed |
| `HardFinal` | it is settled — this is the one to wait for |
| `Rejected` | it will never execute |

```rust
client.wait_for_final(&tx_id, Duration::from_secs(120)).await?;
```

Do not treat a payment as received before `HardFinal`.

## Gas

Every transaction pays a flat fee based on its weight, not on how long it runs:

| Weight | Typical action | Fee |
|---|---|---|
| 1 | transfer, simple token method | $0.01 |
| 2 | mint, burn, stake | $0.05 |
| 3 | | $0.10 |
| 4 | | $0.25 |
| 5 | contract deploy, heavy contract call | $0.50 |

Fees are quoted in USD and charged in RTM at the network's reference price. You can add a priority fee, capped at 1 RTM; it is not part of what you sign.

Inside a contract there is a second budget, **fuel**, which limits how much work one call may do. Running out of fuel fails the call — and the sender still pays the gas fee and uses the nonce. Keep loops bounded.

## Contracts in one paragraph

A contract is a WebAssembly module with one entry point, `on_use`. The chain hands it a call (who is calling, which action, arguments, attached RTM), the contract does its work and returns JSON. Nothing in memory survives between calls: state lives in storage the chain keeps for you. A contract can hold and send its own RTM, but it can never reach into a caller's wallet.

Next: [Quickstart](02-quickstart.md) · [RCP-1](03-rcp-1.md) · [Writing contracts](04-writing-contracts.md)
