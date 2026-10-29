# What Retium is

This page gives you enough of the model to build on Retium with confidence. You do not need to run a node, and you do not need to understand consensus to ship something.

## Accounts

An account is a key pair. The address is derived from the public key and carries a checksum.

```
rete + hex(blake3(public key)) + hex(checksum)     72 characters
```

A mistyped or truncated address fails the checksum, so it is refused rather than paid to. Validate anything a user typed.

```rust
retium_sdk::address::is_valid(&input)
```

Signatures are ML-DSA-44 (FIPS 204), a post-quantum scheme. A key file is 37 bytes, made of a header, a version byte and a 32-byte seed. **The seed is the account.** Back it up and everything else can be rebuilt. The same 64-character seed works in the SDK and in the Retium wallet at [wallet.retium.org](https://wallet.retium.org).

Every transaction is signed with its own key. Key 0 is the seed's own key and gives the address, and key `i` is derived from the seed for the transaction with nonce `i`. Each transaction announces the hash of the next key, and the chain accepts only that key next time, so a key that has signed once is used up. A second derived key, the identity key, signs site logins and never moves money. The SDK does all of this from the one seed, so you only ever handle the seed. See [Transactions](05-transactions.md).

`rete_treasury`, `rete_burn` and `rete_safehouse` look like addresses but are not. They are ledger accounts the chain uses for gas, burns and the reserve, and they can never be a sender.

## Money

**RTM** is the native coin. One RTM is 10^18 atomic units. Everything signed and sent is in atomic units as a decimal string, never a float.

```rust
retium_sdk::amount::parse_rtm("1.5")?;   // 1500000000000000000
retium_sdk::amount::format_rtm(1);       // "0.000000000000000001"
```

**RTD** (Retium Dollar) is the chain's dollar token. Any transaction can pay its gas in RTD instead of RTM, at 1.5 times the RTM fee.

**PIE** is staked RTM. It matters only if you run a validator, and this SDK does not stake.

On the public testnet all three have no value. Nobody should pay for them.

## Who runs the network

Node operators run four kinds of node. You need none of them.

| Role | What it does |
|---|---|
| **Keepers** | Execute transactions, hold the full ledger, agree on blocks |
| **Workers** | Check transactions independently before they become final |
| **Suits** | Confirm that keepers agreed, and finish a block's finality |
| **Readers** | Serve the public RPC, which is what your code talks to |

You reach the network through one address, `https://rpc.retium.org`. You never address keepers directly, and nothing you build needs a node of your own.

## Blocks, ticks and finality

Transactions go into blocks, and blocks belong to ticks. A tick is a round of work. Later ticks carry more blocks than earlier ones.

A transaction moves through these states.

| State | Meaning |
|---|---|
| `Pending` | The network has it. |
| `SoftFinal` | It has executed. |
| `HardFinal` | It is settled. This is the one to wait for. |
| `Rejected` | It will never execute. |
| `Voided` | It will never execute, because another transaction used its nonce. The SDK reports it as `Rejected`. |

A transaction is settled on its own, not with its block. On the public testnet `HardFinal` normally arrives within a second of sending, and a deploy or a call runs at once, so its result is on the transaction by the time it is final.

```rust
let status = client.wait_for_result(&tx_id, Duration::from_secs(120)).await?;
```

Do not treat a payment as received before `HardFinal`. For a deploy or a contract call, also check the result. A call can be final and still have been refused by the contract (see [Transactions](05-transactions.md)).

## Gas

Every transaction pays a flat fee set by its weight, not by how long it runs.

| Weight | Typical action | Fee |
|---|---|---|
| 1 | transfer, simple token method | $0.01 |
| 2 | `mint`, `burn` | $0.05 |
| 3 | standard contract | $0.10 |
| 4 | complex contract | $0.25 |
| 5 | contract deploy, heavy contract call | $0.50 |

Fees are quoted in US dollars and charged in RTM at the network's reference price, or in RTD at 1.5 times that.

Inside a contract there is a second budget, **fuel**, which limits how much work one call may do. Running out of fuel fails the call, and the sender still pays the gas fee and uses the nonce. Keep loops bounded.

## Contracts in one paragraph

A contract is a WebAssembly module with one entry point, `on_use`. The chain hands it a call (who is calling, which action, the arguments, any RTM attached), the contract does its work and returns JSON. Nothing in memory survives between calls. State lives in storage the chain keeps for you. A contract can hold and send its own RTM, but it can never reach into a caller's wallet.

Read on with [Quickstart](02-quickstart.md), [RCP-1](03-rcp-1.md), [Writing contracts](04-writing-contracts.md) or [Wire reference](08-wire-reference.md).
