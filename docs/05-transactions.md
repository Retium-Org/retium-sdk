# Transactions

What you sign, what the node checks, and what can go wrong. You need this page if you are building a wallet, a backend or a bot; the CLI and `tx::*` handle it for you otherwise.

## What a signature covers

Every transaction signs the same structure. The node rebuilds it from the fields you send and compares — so it can validate what you signed, but never rewrite it.

```
signed message = blake3(bincode(payload))

payload = version | chain_id | action | sender | recipient | amount
        | nonce | sender_balance_claim | method | args_hash | declared
```

| Field | Why it is signed |
|---|---|
| `chain_id` | a transaction for one network cannot be replayed on another |
| `nonce` | fixes this transaction's position in your own sequence |
| `sender_balance_claim` | your confirmed balance when you signed |
| `args_hash` | the exact arguments of a contract call |
| `declared` | which accounts this transaction may write to |

Two of these deserve a word.

**The balance claim** is your view of your own balance. Every keeper checks that its own view is at least that, which makes the outcome independent of when other transactions happen to arrive. The SDK fills it in from the node; if you build the payload yourself, do not send zero as a habit — it weakens a check that exists for your protection.

**`declared`** is the list of accounts a transaction may write, sorted and de-duplicated:

| Action | Declared accounts |
|---|---|
| `transfer` | sender, recipient, treasury |
| `deploy` | sender, contract id, treasury |
| `stake` / `unstake` | sender, treasury |
| `call` | empty — the chain works it out, because a call's effects are not known in advance |

If a transaction tries to write outside its declared list, it is refused.

## Building one by hand

```rust
use retium_sdk::{canonical::{CanonicalPayload, ChainId}, key::SecretKey};

let key = SecretKey::read_key_file("wallet.sk")?;
let wallet = client.wallet(&key.address()).await?.unwrap();

let payload = CanonicalPayload::transfer(
    &key.address(),
    recipient,
    retium_sdk::amount::parse_rtm("1.5")?,
    wallet.nonce,
    wallet.balance_atomic()?,
).on_chain(ChainId::Testnet);

let signature = key.sign_hex(&payload.signing_hash());
```

Then post it with `sender_balance_claim`, `nonce`, `declared`, `signature` and `public_key`. The helpers in [`tx`](../crates/retium-sdk/src/tx.rs) do exactly this, and are the better starting point.

Your public key must be included at least on your account's first transaction; the chain remembers it afterwards. Sending it every time is harmless and simpler.

## Nonces

The nonce is your account's sequence number. Ask the node for the next one — do not guess, and do not cache it across a process restart:

```rust
let nonce = client.next_nonce(&address).await?;
```

If a transaction is refused for a nonce, something else spent from the account in between. Fetch the nonce again and rebuild. Never reuse a nonce for different content.

Parallel sends from one account are a trap: two transactions with the same nonce mean one of them dies. Send from one place, in order, or use separate accounts.

## Gas

A flat fee by weight (weight 1 = $0.01 … weight 5 = $0.50), charged in RTM at the network's reference price. You can add a priority fee, capped at 1 RTM. The fee is **not** part of what you sign, so it cannot be used to change your transaction.

Keep a little RTM spare: a call that fails still costs its fee.

## Finality

| State | Meaning |
|---|---|
| `Pending` | the network has it |
| `SoftFinal` | it has executed |
| `HardFinal` | settled |
| `Rejected` | it will never execute |

```rust
match client.wait_for_final(&tx_id, Duration::from_secs(300)).await? {
    Finality::HardFinal => println!("settled"),
    Finality::Rejected => println!("refused by the network"),
    _ => unreachable!(),
}
```

Do not credit a user, ship goods or chain a dependent transaction before `HardFinal`.

## Errors, and which ones to retry

| Error | Retry? | What it means |
|---|---|---|
| `Busy` | **yes** | the node had no open block or was mid-handover; the same bytes may go through in a moment |
| `Transport` | **yes** | the request never got an answer |
| `Refused` | no | the node looked at it and said no — fix the transaction |
| `InvalidAddress`, `InvalidAmount` | no | caught locally before anything was sent |

```rust
if err.is_retryable() { /* back off and send the same transaction again */ }
```

Retrying a refused transaction only wastes gas. Retrying a busy one is normal and expected.

## Reading without transacting

Anything that does not change state should not be a transaction:

```rust
client.query(contract_id, "balance_of", &[user.to_string()]).await?;  // free
```

This runs the contract read-only. No gas, no nonce, no waiting.

Next: [Verifying code](06-verifying-code.md) · [Security](07-security.md) · [Wire reference](08-wire-reference.md) if you are building outside Rust
