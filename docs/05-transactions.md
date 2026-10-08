# Transactions

This page covers what you sign, what the node checks, and what can go wrong. You need it if you are building a wallet, a backend or a bot. Otherwise the CLI and `tx::*` handle it for you.

## What a signature covers

Every transaction signs the same structure. The node rebuilds it from the fields you send and compares, so it can check what you signed but never rewrite it.

```
signed message = blake3(bincode(payload))

payload = version | chain_id | action | sender | recipient | amount
        | nonce | sender_balance_claim | method | args_hash | declared
        | fee_currency
```

| Field | Why it is signed |
|---|---|
| `chain_id` | A transaction for one network cannot be replayed on another. |
| `nonce` | It fixes this transaction's place in your own sequence. |
| `sender_balance_claim` | Your confirmed balance when you signed. |
| `args_hash` | The exact arguments of a contract call. |
| `declared` | Which accounts this transaction may write to. |
| `fee_currency` | Whether gas is paid in RTM or RTD, so a relay cannot change it. |

Every network signs with chain id 3 until mainnet, the public testnet included. The SDK uses that by default, and a transaction signed for chain id 2 is refused today.

**The balance claim** is your view of your own balance. Every keeper checks that its own view is at least that much, which makes the result independent of when other transactions arrive. The SDK fills it in from the node. If you build the payload yourself, do not send zero as a habit, because that weakens a check that exists to protect you.

**`declared`** is the list of accounts a transaction may write, sorted and de-duplicated.

| Action | Declared accounts |
|---|---|
| `transfer` | sender, recipient, treasury |
| `deploy` | sender, contract id, treasury |
| `api_key` | sender, treasury |
| `call` | none, because a call's effects are not known in advance and the chain works them out |

A transaction that tries to write outside its declared list is refused.

## Building one by hand

```rust
use retium_sdk::{canonical::CanonicalPayload, key::SecretKey};

let key = SecretKey::read_key_file("wallet.sk")?;
let wallet = client.wallet(&key.address()).await?.unwrap();

let payload = CanonicalPayload::transfer(
    &key.address(),
    recipient,
    retium_sdk::amount::parse_rtm("1.5")?,
    wallet.nonce,
    wallet.balance_atomic()?,
);

let signature = key.sign_hex(&payload.signing_hash());
```

Then post it with `nonce`, `sender_balance_claim`, `declared`, `fee_currency`, `signature` and `public_key`. The helpers in [`tx`](../crates/retium-sdk/src/tx.rs) do exactly this and are the better starting point. The [Wire reference](08-wire-reference.md) lists every field.

Include your public key at least on your account's first transaction, after which the chain remembers it. Sending it every time is harmless and simpler.

## Call arguments

A call signs `blake3` of its arguments joined with commas, and the chain splits them back apart on the same commas to run the call. An argument containing `,` `(` `)` or `::` would reach the contract changed, while the signature stayed valid. `tx::call` refuses such arguments before signing, and so does `retium call`. See [Writing contracts](04-writing-contracts.md#arguments).

## Nonces

The nonce is your account's sequence number. Ask the node for the next one. Do not guess, and do not cache it across a restart.

```rust
let nonce = client.next_nonce(&address).await?;
```

If a transaction is refused for its nonce, something else spent from the account in between. Fetch the nonce again and rebuild. Never reuse a nonce for different content.

Parallel sends from one account are a trap. Two transactions with the same nonce mean one of them dies. Send from one place, in order, or use separate accounts.

## Gas

A flat fee by weight, from $0.01 at weight 1 to $0.50 at weight 5. It is charged in RTM at the network's reference price, or in RTD at 1.5 times that.

```rust
let opts = retium_sdk::tx::Options::default().gas_in_rtd();
tx::transfer_with(&client, &key, to, "1.5", opts).await?;
```

On the command line add `--fee RTD`. The choice of coin is signed. The amount is not part of what you sign, but the node computes it from the weight and the coin, so it cannot be raised behind your back.

Keep a little spare. A call the contract refuses still costs its fee.

## Finality and results

| State | Meaning |
|---|---|
| `Pending` | The network has it. |
| `SoftFinal` | It has executed. |
| `HardFinal` | It is settled. |
| `Rejected` | It will never execute. |
| `Voided` | It will never execute, because another transaction used its nonce. The SDK reports it as `Rejected`. |

A deploy or a call has a second answer, its result. The transaction can be `HardFinal` while the contract refused it. In that case nothing it did is kept, any RTM attached is returned, and the gas is still paid.

```rust
use retium_sdk::rpc::{Finality, Outcome};

let status = client.wait_for_result(&tx_id, Duration::from_secs(300)).await?;
match (status.finality, status.outcome) {
    (Finality::HardFinal, Outcome::Ok | Outcome::NotApplicable) => println!("settled"),
    (_, Outcome::Failed(reason)) => println!("refused: {reason}"),
    (other, _) => println!("ended as {other:?}"),
}
```

`retium tx <id> --wait` prints the same and exits with an error for a refused call. Do not credit a user, ship goods or send a dependent transaction before `HardFinal` and a good result.

## Errors and retries

| Error | Retry? | What it means |
|---|---|---|
| `Busy` | **yes** | The node had no open block, was mid-handover or was over its rate limit. The same bytes may go through in a moment. |
| `Transport` | **yes** | The request never got an answer. |
| `Refused` | no | The node looked at it and said no. Fix the transaction. |
| `Invalid`, `InvalidAddress`, `InvalidAmount` | no | Caught on your machine, nothing was sent. |

```rust
if err.is_retryable() { /* wait, then send the same transaction again */ }
```

Retrying a refused transaction only wastes time. Retrying a busy one is normal. [Errors](11-errors.md) lists the messages you will meet.

## Reading without a transaction

Anything that does not change state should not be a transaction.

```rust
client.query(contract_id, "balance_of", &[user.to_string()]).await?;
```

This runs the contract read-only, with no gas, no nonce, no API key and no waiting. The read travels in the URL and the node does not decode it, so `query` takes arguments made of letters, digits and `- _ . ~ :`. Addresses, ids and numbers all fit. For other text use `client.view`, which sends the arguments as JSON and needs an API key on the public endpoint.

Read on with [Verifying code](06-verifying-code.md), [Security](07-security.md) or [Wire reference](08-wire-reference.md) if you build outside Rust.
