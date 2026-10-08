# Errors and troubleshooting

What the messages mean and what to do about them. The short version is to retry when the network is busy, and to fix the transaction when it is refused.

## Three kinds of failure

1. **Caught on your machine.** The SDK checks addresses, amounts, call arguments and deploys before it signs anything. Nothing was sent and nothing was spent. The message says what to change.
2. **Refused by the node.** The node looked at the transaction and said no. Nothing was spent. Sending the same bytes again gives the same answer.
3. **Refused by the contract.** The transaction was accepted and became final, but the contract said no. Everything it did was undone and attached RTM was returned, but the gas was paid. `retium tx <id> --wait` shows the reason and exits with an error.

## HTTP status codes

| Status | Meaning | Retry? |
|---|---|---|
| 400 | refused, see the message | no, fix it |
| 401 | no API key on a write | no, set `RETIUM_API_KEY` |
| 403 | the API key is unknown or revoked | no, check the key |
| 404 | no such account record, contract or transaction yet | for a fresh transaction, poll again |
| 413 | contract over 1 MiB | no, build with `--release` |
| 429 | over your API key's rate limit | yes, after `Retry-After` |
| 503 | no open block, a handover, or busy | yes, the same bytes |

The SDK turns 429 and 503 into `Error::Busy` and says the request is worth trying again. 400, 401, 403 and 413 become `Error::Refused`.

## Messages you will meet

| Message | Cause | Fix |
|---|---|---|
| `Nonce mismatch: got 4 but expected 5` | Another transaction from this account went first. | Send again. The SDK reads a fresh nonce each time. Do not send from one account in parallel. |
| `Insufficient balance` | Not enough RTM for the amount plus gas. | Fund the account, or pay gas in RTD with `--fee RTD`. |
| `gas in RTD: ...` | You chose RTD gas without enough RTD. | Fund RTD, or pay in RTM. |
| `Signature verification failed` | The signed fields and the sent fields differ. | Use the SDK helpers. If you build requests yourself, see [Wire reference](08-wire-reference.md). |
| `Missing X-API-Key header` | A write without a key. | Create one at [retium.net/api-keys](https://retium.net/api-keys). |
| `unknown developer key` or `developer key revoked` | The key was mistyped, is not final yet, or was revoked. | Check `retium api-key list`. A new key works once its transaction is final. |
| `developer key over quota` | The key's daily or per-second limit. | Wait, or buy a bigger tier with `retium api-key upgrade`. |
| `No Open blocks`, `tx_not_delivered` | The network is between blocks or handing over. | Send the same transaction again in a few seconds. |
| `busy: all contract view slots are in use, retry` | Too many free reads at once on that node. | Try again shortly. |
| `Contract too large` | Over 1 MiB. | Build with `--release`. Split a contract that really is that big. |
| `contract refused the call: ...` | The contract returned an error. The rest is its own message. | Read the contract's message. |
| `init on a token runs only at deploy` | Tokens are initialised by the chain at deploy. | Nothing to do. See [Tokens](09-tokens.md). |

## Deploys

`retium deploy --dry-run` runs every check the chain runs on a deploy and reports the first one that fails. The common ones are listed here.

| Message | Fix |
|---|---|
| `embedded RCP-1 metadata is not valid (missing field ...)` | Give all seven metadata fields. Use `rcp_metadata!`. |
| `declared weight 1 does not fit this code` | Declare weight 5 for a Rust contract. The message gives the allowed values. |
| `the wasm has no 'on_use' export` | Use `entrypoint!`, and build the crate as a `cdylib`. |
| `the name "RTD" is reserved` | Choose another name or symbol. |
| `contains invisible or text-direction characters` | Remove them from the name, symbol or traits. |
| `decimals 40 out of range (0-38)` | Use 0 to 38. |


## Calls

| Message from the SDK | Fix |
|---|---|
| `argument 1 ("a,b") contains , ( ) or ::` | Split it into several arguments or encode it. |
| `a single empty argument reaches the contract as no arguments` | Pass no arguments instead. |
| `argument ... cannot be sent in a free read` | Use `retium view` or `Client::view` for that text. |

## When a transaction seems stuck

A transaction stays `Pending` while the network works through its block. Later ticks carry more blocks, so waiting a few minutes is normal. `retium tx <id> --wait` keeps polling for up to ten minutes. A transaction that ends `Voided` will never run, because another transaction used its nonce. Send it again with a fresh nonce.

Still stuck? Ask in the Retium Discord with the transaction id. Never post your seed or key file anywhere.
