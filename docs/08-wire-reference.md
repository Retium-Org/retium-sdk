# Wire reference

Everything needed to talk to Retium from any language. The HTTP interface, the exact bytes a signature covers, and the contract interface.

If you write Rust, use [`retium-sdk`](../crates/retium-sdk) instead. It does all of this and is kept in step with the network. This page is for people building in another language, or checking what the SDK sends.

## 1. Endpoint

One base URL, `https://rpc.retium.org`.

Reads are open and need no key. They are rate limited per address. Every `POST` needs an `X-API-Key` header with a developer key (`rk_...`), which you create at [retium.net/api-keys](https://retium.net/api-keys). The key sets your rate limit and grants no authority over any account. Authority comes only from your signature.

## 2. Accounts

**Keys** are ML-DSA-44 (FIPS 204), and nothing else is accepted.

| Thing | Size | Notes |
|---|---|---|
| Seed (the secret) | 32 bytes | the account, everything else derives from it |
| Public key | 1,312 bytes | sent as hex |
| Signature | 2,421 bytes | `0x02` then the 2,420-byte ML-DSA-44 signature, sent as hex |
| Key file | 37 bytes | `"RTSK"`, then `0x02`, then the seed |

The key pair is FIPS 204 `ML-DSA.KeyGen_internal` from the 32-byte seed. The Retium wallet uses the same seed, so a key moves between the two by its 64 hex characters.

**Address**

```
H        = blake3(public key)                          32 bytes
C        = first 2 bytes of blake3("retium.addr.v1" + H)
address  = "rete" + hex(H) + hex(C)                    72 characters, lowercase
```

Validate any address you did not derive yourself by recomputing `C`.

`rete_treasury`, `rete_burn` and `rete_safehouse` are ledger accounts, not addresses. They never validate and can never be a sender.

## 3. What a signature covers

The signed message is `blake3(bincode(payload))`, and the payload has these fields in this order.

| # | Field | Type |
|---|---|---|
| 1 | `version` | `u8`, currently `5` |
| 2 | `chain_id` | `u32`, 1 mainnet, 2 testnet, 3 devnet. Every network uses `3` until mainnet, the public testnet included. |
| 3 | `action` | string, `transfer`, `deploy`, `call` or `api_key` |
| 4 | `sender` | string, your address |
| 5 | `recipient` | string, an address, a contract id or `API_KEY_REGISTRY` |
| 6 | `amount` | `u128`, atomic units |
| 7 | `nonce` | `u64` |
| 8 | `sender_balance_claim` | `u128`, your confirmed balance when signing |
| 9 | `method` | string, the contract method for `call`, otherwise empty |
| 10 | `args_hash` | 32 bytes, see below |
| 11 | `declared` | list of strings, see below |
| 12 | `fee_currency` | `u8`, the coin the gas is paid in, `0` RTM or `1` RTD (1.5 times the RTM fee) |

### Encoding

Bincode with fixed-width integers, little-endian.

* An integer is little-endian at its natural width (`u8` 1 byte, `u32` 4, `u64` 8, `u128` 16).
* A string is its byte length as a little-endian `u64`, then the UTF-8 bytes.
* `[u8; 32]` is the 32 bytes with no length prefix.
* A list is its count as a little-endian `u64`, then each element encoded as above.

A `transfer` of 1 RTM with nonce 0 therefore begins like this.

```
05                          version 5
03 00 00 00                 chain_id 3
08 00 00 00 00 00 00 00     "transfer" is 8 bytes
74 72 61 6e 73 66 65 72     "transfer"
48 00 00 00 00 00 00 00     sender is 72 bytes
...                         sender
...                         recipient (72 bytes, same shape)
00 00 64 a7 b3 b6 e0 0d     amount, u128 little-endian, first 8 of 16 bytes
...                         the other 8 bytes of amount
00 00 00 00 00 00 00 00     nonce 0
...                         sender_balance_claim, u128
00 00 00 00 00 00 00 00     method "" is 0 bytes
00 x32                      args_hash
03 00 00 00 00 00 00 00     declared has 3 entries
...                         each entry is a u64 length then the bytes
00                          fee_currency 0 (RTM)
```

### `declared`

The accounts a transaction may write, **sorted by byte order and de-duplicated**.

| Action | Contents |
|---|---|
| `transfer` | sender, recipient, `rete_treasury` |
| `deploy` | sender, contract id, `rete_treasury` |
| `api_key` | sender, `rete_treasury` |
| `call` | empty, the chain derives it |

A wrong list is refused before the signature is even considered.

### `args_hash`

For a `call`, `blake3` of the arguments joined with commas, exactly as sent.

```
args = ["rete...bob", "10"]      gives blake3("rete...bob,10")
```

The chain carries the call as `contract::method(arg1,arg2)` and splits it on the same commas to run it. So an argument must not contain `,` `(` `)` or `::`, a method name is letters, digits and `_`, and a call with exactly one empty argument arrives with none. Refuse these before signing, because the signature would still be valid for the changed arguments.

For `api_key`, `blake3` of the payload JSON text, byte for byte as sent. For `transfer` and `deploy`, 32 zero bytes.

## 4. HTTP interface

### Reads

| Request | Answer |
|---|---|
| `GET /health` | `{status, tick, peers, version}` |
| `GET /api/wallet/{address}` | `{address, balance_rtm, balance_pie, pending_incoming, nonce, ...}`. An account the chain has not seen answers with zero balances and nonce 0. |
| `GET /api/wallet/{address}/tokens` | `{tokens: [{contract_id, name, symbol, decimals, balance}]}`, balance in the token's smallest unit |
| `GET /api/transaction/{tx_id}` | `{finality, exec_status, exec_error, action, block_id, gas_breakdown, ...}` |
| `GET /api/contract/{id}` | `{contract_id, deployer, code_hash, rcp_metadata, ...}`, 404 if there is no such contract |
| `GET /api/contract/{id}/query?method=M&args=a,b` | a free read-only call, `{ok, contract_id, method, output, view}` |
| `GET /api/contract/{id}/events?after=0&limit=100` | events in the order they were applied, `{events, next, more}`. Ask again with `after=next` until `more` is false. `type=` filters by event type. |
| `GET /api/contract/{id}/storage/{key}` | one storage value |
| `GET /api/developer/tiers` | the API key price list |
| `GET /api/developer/keys/{address}` | the keys an address registered, by hash |

`balance_rtm` is decimal RTM, and `nonce` is the next nonce to sign. `finality` is one of `Pending`, `SoftFinal`, `HardFinal`, `Rejected` and `Voided`. For a deploy or a call that is `HardFinal`, `exec_status` is `ok`, `failed` (with the reason in `exec_error`) or `pending` while the contract has not run yet.

The query endpoint does not decode its URL. Send the method and the comma-joined arguments as they are, using only letters, digits and `- _ . ~ :`. For anything else use a view (below).

### Writes

All writes are `POST`, JSON in and JSON out, with an `X-API-Key` header. These fields are common to all of them.

| Field | Type | Notes |
|---|---|---|
| `nonce` | number | must equal the account's next nonce |
| `sender_balance_claim` | string | atomic units |
| `declared` | array of strings | as above |
| `fee_currency` | string | `"RTM"` (the default when absent) or `"RTD"`, and it must match the signed byte |
| `signature` | string | hex, 4,842 characters |
| `public_key` | string | hex, required on an account's first transaction, harmless afterwards |

**`POST /api/wallet/send`**

```json
{"from": "rete...", "to": "rete...", "amount": "1500000000000000000",
 "nonce": 0, "sender_balance_claim": "25000000000000000000",
 "declared": ["rete...", "rete...", "rete_treasury"], "fee_currency": "RTM",
 "signature": "02...", "public_key": "..."}
```

`amount` as a string is atomic units. Always send a string.

**`POST /api/contract/deploy`**

```json
{"from": "rete...", "wasm_base64": "AGFzbQEAAAA...",
 "metadata": {"name": "My Token", "symbol": "MTK"},
 "nonce": 3, "sender_balance_claim": "...",
 "declared": ["rete...", "<contract id>", "rete_treasury"], "fee_currency": "RTM",
 "signature": "02...", "public_key": "..."}
```

`metadata` is optional, is part of the contract id, and is at most 8 KiB. The answer is `{tx_id, contract_id, status, block_id}`. A module over **1 MiB** is refused with HTTP 413.

**`POST /api/contract/call`**

```json
{"from": "rete...", "contract_id": "...", "method": "bump", "args": ["5"],
 "amount": "0", "nonce": 4, "sender_balance_claim": "...",
 "declared": [], "fee_currency": "RTM", "signature": "02...", "public_key": "..."}
```

`amount` attaches RTM to the call. Leave it out for none.

A **view** is the same endpoint with `"view": true`, `contract_id`, `method`, `args` and `"nonce": 0`, and nothing signed. It runs read-only, answers at once with `{ok, output, error}` and costs nothing, but it is a `POST`, so it needs the API key. Its arguments travel as a JSON array, so any text is allowed.

**`POST /api/developer/key`** registers, renames, revokes or upgrades an API key. The payload is JSON text, signed through `args_hash`.

```json
{"address": "rete...",
 "payload": "{\"op\":\"register\",\"key_hash\":\"<64 hex>\",\"label\":\"my bot\"}",
 "amount": "0", "nonce": 5, "sender_balance_claim": "...",
 "declared": ["rete...", "rete_treasury"], "fee_currency": "RTM",
 "signature": "02...", "public_key": "..."}
```

The secret is `rk_` followed by 64 hex characters, chosen by you. Only `key_hash`, the hex `blake3` of the secret, goes on chain. `op` is `register` or `label` (with `label`, 1 to 32 characters), `revoke`, or `tier` (with `tier`, `pro` or `enterprise`). A `tier` op signs the tier's `price_atomic` from `/api/developer/tiers` as its `amount`, and every other op signs 0. The recipient in the signed payload is `API_KEY_REGISTRY`.

### Answers and status codes

Success is `200` with a `tx_id`. An accepted transaction has *not* settled yet. Poll `GET /api/transaction/{tx_id}` until `finality` is final, then read `exec_status`.

| Status | Meaning | Retry? |
|---|---|---|
| 400 | refused for its signature, nonce, balance or format | no |
| 401 | no API key | no |
| 403 | unknown or revoked API key | no |
| 404 | no such contract or transaction (yet) | for a fresh transaction, yes |
| 413 | contract too large | no |
| 429 | over your rate limit, see `Retry-After` | **yes**, later |
| 503 | the node had no open block, was mid-handover or was busy | **yes**, same bytes |

## 5. Gas

A flat fee by weight, charged in RTM at the network's reference price, or in RTD at 1.5 times that.

| Weight | Fee | Applies to |
|---|---|---|
| 1 | $0.01 | transfers and API key ops, and the methods `transfer`, `transfer_from`, `approve`, `allowance`, `balance_of`, `name`, `symbol`, `decimals`, `total_supply` |
| 2 | $0.05 | the methods `mint` and `burn` |
| 3 | $0.10 | |
| 4 | $0.25 | |
| 5 | $0.50 | deploys, and contracts that declare weight 5 |

Any other contract method is charged at the weight the contract declared in its RCP-1 metadata.

## 6. Contract interface

A contract is a wasm module that exports `memory` and one function.

```
on_use(input_ptr: i32, input_len: i32, output_ptr: i32) -> i32
```

* The input JSON is written at offset 0 of your memory. `input_ptr` is 0 and `input_len` is its length.
* Write your reply at `output_ptr` and return its length in bytes.
* A **negative** return means failure.
* A **trap** discards everything the call did, its writes, events and transfers.
* The reply must be valid UTF-8 and valid JSON, at most **4,096 bytes**. A reply that is a JSON object with an `error` field is a refusal. On a user's call the chain then undoes the call like a trap and keeps the error as the reason.

The input JSON looks like this.

```json
{"action": "bump", "caller": "rete...", "args": ["5"], "token_context": null, "value": "0"}
```

`caller` is proven by the chain. `value` is the RTM attached to this call, in atomic units, as a decimal string, and always present.

### Host functions

Imported from module `env`. Pointers are offsets into your memory and every length is in bytes.

| Function | Signature | Returns |
|---|---|---|
| `write_state` | `(key_ptr, key_len, val_ptr, val_len)` | nothing. An empty value makes the key read as not found. Traps over 64 KiB. |
| `read_state` | `(key_ptr, key_len, out_ptr, out_max) -> i32` | bytes written, `0` not found, `-1` error |
| `list_keys` | `(prefix_ptr, prefix_len, out_ptr, out_max) -> i32` | bytes of a JSON array of keys, cut off at `out_max`. A key written empty is still listed. |
| `emit_event` | `(type_ptr, type_len, data_ptr, data_len) -> i32` | `0` or `-1`. Type up to 64 B, data up to 4 KiB. |
| `get_self_contract_id` | `(out_ptr, out_max) -> i32` | bytes written |
| `get_block_height` | `() -> i64` | block id |
| `get_block_time` | `() -> i64` | block timestamp, ms |
| `sha256` | `(in_ptr, in_len, out_ptr) -> i32` | `32`, writing 32 bytes |
| `sig_verify` | `(msg_ptr, msg_len, sig_ptr, sig_len, pk_ptr, pk_len) -> i32` | `1` valid, `0` invalid, `-1` error |
| `call_contract` | `(id_ptr, id_len, method_ptr, method_len, args_ptr, args_len, out_ptr, out_max) -> i32` | bytes written, `-1` failed |
| `call_contract_value` | `(id_ptr, id_len, method_ptr, method_len, args_ptr, args_len, amount_ptr, amount_len, out_ptr, out_max) -> i32` | the same. The amount is a decimal string. |
| `get_contract_code_hash` | `(id_ptr, id_len, out_ptr, out_max) -> i32` | 64 hex characters |
| `get_contract_version` | `(id_ptr, id_len, out_ptr, out_max) -> i32` | the version a contract declares |
| `set_contract_version` | `(ver_ptr, ver_len) -> i32` | `0` or `-1`, up to 64 bytes |
| `supports_capability` | `(id_ptr, id_len, cap_ptr, cap_len) -> i32` | `1`, `0` or `-1`. A claim, not a guarantee. |

There is no clock, no randomness, no network and no file access. `call_contract` and `call_contract_value` take arguments as a JSON array of strings, for example `["rete...","10"]`, and answer `-1` for anything else. The contract you call receives them in its own `args`. When the contract you call answers with an error reply, you get that reply, and its writes stay unless your call fails too.

### Execution rules

* Module globals do **not** persist, because the module is rebuilt for every call. All state goes through `write_state`.
* Writes land in an overlay that is kept when the call succeeds and discarded when it traps or is refused.
* A contract cannot call itself, directly or in a circle. Calls nest at most 64 deep.
* Work is metered in fuel, 100,000,000 per transaction. Running out fails the call, and the sender still pays gas and uses the nonce. A cross-contract call costs 50,000 and `sig_verify` 80,000 before anything else.
* A transaction may run for 5 seconds and a free read for 2.
* A contract can send its own RTM but can never take RTM from the caller's wallet. The only money a caller gives you is `value`, which they signed.

### Limits

| | |
|---|---|
| Contract size | 1 MiB |
| Reply | 4 KiB |
| Storage key | 1 KiB |
| Storage value | 64 KiB per key |
| Event type and data | 64 B and 4 KiB |
| Version string | 64 B |
| Memory | 32 MiB |
| Call depth | 64 |

## 7. RCP-1 metadata and contract identity

Embedded in the wasm as a marker, JSON and a zero byte. All seven fields are required.

```
RCP_METADATA:{"rcp_type":"token","traits":["fungible"],"capabilities":["execute"],"lockable_fields":[],"evolves_from":"","weight":5,"version":"v1.0.0"}\0
```

The chain checks at deploy that `rcp_type` and `version` are not empty, that there is at least one trait or capability, and that `weight` is between 1 and 5 and equal to the chain's score of the code or one above it. Rust contracts score 5. A module must export `on_use`.

```
contract_id = hex(blake3( wasm + deployer + [weight] + name? + symbol? + rcp_type? + each trait ))
code_hash   = hex(blake3( wasm ))
```

`weight` is `5` for every deploy through the RPC. The optional fields come from the deploy request's `metadata` object, appended only when present, in that order. The chain computes `code_hash` itself, and it is the one part of a contract's identity nobody can declare. Everything else in the metadata is a claim by whoever deployed it. See [RCP-1](03-rcp-1.md) for how deploy metadata overrides the embedded record, and [Verifying code](06-verifying-code.md) for how to use the code hash.
