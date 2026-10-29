# Transactions

This page covers what you sign, what the node checks, and what can go wrong. You need it if you are building a wallet, a backend or a bot. Otherwise the CLI and `tx::*` handle it for you.

## What a signature covers

Every transaction signs the same structure. The node rebuilds it from the fields you send and compares, so it can check what you signed but never rewrite it.

```
signed message = blake3(bincode(payload))

payload = version | chain_id | action | sender | recipient | amount
        | nonce | sender_balance_claim | method | args_hash | declared
        | fee_currency | next_key_hash | login_key_hash | cosign_key_hash
```

| Field | Why it is signed |
|---|---|
| `chain_id` | A transaction for one network cannot be replayed on another. |
| `nonce` | It fixes this transaction's place in your own sequence. |
| `sender_balance_claim` | Your confirmed balance when you signed. |
| `args_hash` | The exact arguments of a contract call. |
| `declared` | Which accounts this transaction may write to. |
| `fee_currency` | Whether gas is paid in RTM or RTD, so a relay cannot change it. |
| `next_key_hash` | The hash of the key you will sign your next transaction with. |
| `login_key_hash`, `cosign_key_hash` | The hashes of your identity key and of your co-signing key, published once. |

The public testnet uses chain id 3 and payload version 6, the SDK defaults.

## One key per transaction

Every transaction is signed with a key of its own. Key 0 is the seed's own key, and its hash is your address. Key `i` is derived from the seed, and it signs the transaction with nonce `i`. The transaction carries the public key it was signed with and the hash of the next key. The chain keeps that hash and accepts only that key for your next transaction, so a key that has signed once is used up, and a public key seen on the chain is worthless to anyone who copies it.

```rust
let signing = key.key_at(nonce);                         // the key for this transaction
let next = key_hash(&key.key_at(nonce + 1).public_key()); // what the chain expects after it
```

Two more keys come from the same seed. The **identity key** (`key.identity_key()`) signs site logins and messages and can never move money. The **co-signing key** (`key.cosign_key(counter)`) is used only when your wallet is a member of a 2FA or multisig vault. Your first transaction publishes the hashes of both, and later transactions send zeros to leave them as they are.

Before signing, read `GET /api/wallet/{address}`. Its `next_key_hash` is the key the chain expects, and `nonce` is its index. A wallet that has never sent anything has no `next_key_hash`, and key 0 signs. `tx::*` does all of this and refuses to sign when the seed does not produce the key the chain expects.

**The balance claim** is your confirmed balance when you signed. A keeper refuses the transaction if its own view of your balance is below the claim, so the result does not depend on when other transactions arrive. The SDK fills it in from the node. If you build the payload yourself, send the balance the node reported.

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

use retium_sdk::key::key_hash;

let next = key_hash(&key.key_at(wallet.nonce + 1).public_key());
let login = wallet.login_key_hash.is_none().then(|| key_hash(&key.identity_key().public_key()));
let cosign = wallet.cosign.is_none().then(|| key_hash(&key.cosign_key(0).public_key()));
let payload = payload.with_key_hashes(next, login, cosign);

let signing = key.key_at(wallet.nonce);
let signature = signing.sign_hex(&payload.signing_hash());
let public_key = signing.public_key().to_hex();
```

Then post it with `nonce`, `sender_balance_claim`, `declared`, `fee_currency`, `signature`, `public_key`, `next_key_hash` and, on the first transaction, `login_key_hash` and `cosign_key_hash`, all hashes as hex. The helpers in [`tx`](../crates/retium-sdk/src/tx.rs) do exactly this and are the better starting point. The [Wire reference](08-wire-reference.md) lists every field.

`public_key` is the key of this transaction, so it is different every time and always required.

## Call arguments

A call signs `blake3` of its arguments joined with commas. An argument may not contain `,` `(` `)` or `::`. `tx::call` and `retium call` check this before signing. See [Writing contracts](04-writing-contracts.md#arguments).

## Nonces

The nonce is your account's sequence number. Ask the node for the next one. Do not guess, and do not cache it across a restart.

```rust
let nonce = client.next_nonce(&address).await?;
```

If a transaction is refused for its nonce, something else spent from the account in between. Fetch the nonce again and rebuild with the key of the new nonce. Never reuse a nonce for different content.

Parallel sends from one account are a trap. Two transactions with the same nonce mean one of them dies, and the node refuses the second one it sees with `Nonce N is already in flight`. Send from one place, in order, or use separate accounts.

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

A transaction is settled on its own, not with its block. On the public testnet `HardFinal` normally arrives within a second, and a deploy or a call runs at once, so its result is there by the time it is final.

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
| `Busy` | **yes** | The node had no open block, was mid-handover, was over its rate limit, or answered `rpc busy, retry` under load. The same bytes may go through in a moment. |
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
