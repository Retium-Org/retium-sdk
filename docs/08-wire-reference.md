# Wire reference

Everything needed to talk to Retium from any language: the HTTP interface, the exact bytes a signature covers, and the contract interface.

If you are writing Rust, use [`retium-sdk`](../crates/retium-sdk) instead — it does all of this and is kept in step with the network. This page is for people building in another language, or auditing what the SDK sends.

## 1. Endpoint

One base URL: `https://rpc.retium.org`.

Reads are open. Writes want an `X-API-Key` header; the key raises your rate limit and grants no authority over any account — authority comes from your signature.

## 2. Accounts

**Key**: ML-DSA-44 (FIPS 204), and nothing else is accepted.

| Thing | Size | Notes |
|---|---|---|
| Seed (the secret) | 32 bytes | the account; everything else derives from it |
| Public key | 1,312 bytes | sent as hex |
| Signature | 2,421 bytes | `0x02` ‖ 2,420-byte ML-DSA-44 signature, sent as hex |
| Key file | 37 bytes | `"RTSK"` ‖ `0x02` ‖ seed |

**Address**:

```
H        = blake3(public key)                          32 bytes
C        = blake3("retium.addr.v1" ‖ H)[0..2]           2 bytes
address  = "rete" ‖ hex(H) ‖ hex(C)                    72 chars, lowercase
```

Validate any address you did not derive yourself by recomputing `C`.

`rete_treasury` and `rete_burn` are ledger accounts, not addresses. They never validate and can never be a sender.

## 3. What a signature covers

The signed message is `blake3(bincode(payload))`, and the payload is:

| # | Field | Type |
|---|---|---|
| 1 | `version` | `u8` — currently `4` |
| 2 | `chain_id` | `u32` — 1 mainnet, 2 testnet, 3 devnet |
| 3 | `action` | string — `transfer`, `deploy`, `call`, `stake`, `unstake` |
| 4 | `sender` | string — your address |
| 5 | `recipient` | string — address, contract id, or `STAKE_POOL` |
| 6 | `amount` | `u128` — atomic units |
| 7 | `nonce` | `u64` |
| 8 | `sender_balance_claim` | `u128` — your confirmed balance when signing |
| 9 | `method` | string — contract method for `call`, else empty |
| 10 | `args_hash` | 32 bytes — `blake3(args)` for `call`, else all zeros |
| 11 | `declared` | list of strings — see below |

### Encoding

Bincode with fixed-width integers, little-endian:

* integers: little-endian, their natural width (`u8` 1 byte, `u32` 4, `u64` 8, `u128` 16);
* string: `u64` little-endian byte length, then the UTF-8 bytes;
* `[u8; 32]`: the 32 bytes, with no length prefix;
* list: `u64` little-endian count, then each element encoded as above.

A `transfer` of 1 RTM, nonce 0, therefore begins:

```
04                          version 4
03 00 00 00                 chain_id 3
08 00 00 00 00 00 00 00     "transfer" is 8 bytes
74 72 61 6e 73 66 65 72     "transfer"
48 00 00 00 00 00 00 00     sender is 72 bytes
…                           sender
…                           recipient (72 bytes, same shape)
00 00 64 a7 b3 b6 e0 0d …   amount, u128 little-endian
00 00 00 00 00 00 00 00     nonce 0
…                           sender_balance_claim, u128
00 00 00 00 00 00 00 00     method "" is 0 bytes
00 ×32                      args_hash
03 00 00 00 00 00 00 00     declared has 3 entries
…                           each: u64 length + bytes
```

### `declared`

The accounts a transaction may write, **sorted by byte order and de-duplicated**:

| Action | Contents |
|---|---|
| `transfer` | sender, recipient, `rete_treasury` |
| `deploy` | sender, contract id, `rete_treasury` |
| `stake`, `unstake` | sender, `rete_treasury` |
| `call` | empty — the chain derives it |

A wrong list is refused before the signature is even considered.

### `args_hash` for a call

`blake3` of the arguments joined with commas, exactly as sent:

```
args = ["rete…bob", "10"]      →  blake3("rete…bob,10")
```

## 4. HTTP interface

### Reads

| Request | Answer |
|---|---|
| `GET /health` | `{status, tick, peers, version}` |
| `GET /api/wallet/{address}` | `{address, balance_rtm, balance_pie, nonce, …}`, 404 if unknown |
| `GET /api/transaction/{tx_id}` | includes `finality`: `SoftFinal`, `HardFinal`, `Rejected` |
| `GET /api/contract/{id}` | `{contract_id, code_hash, rcp_metadata}` |
| `GET /api/contract/{id}/query?method=&args=` | read-only call, no gas |
| `GET /api/contract/{id}/events?type=&from_tick=&limit=` | a contract's events |
| `GET /api/contract/{id}/storage/{key}` | one storage value |

`balance_rtm` is decimal RTM; `nonce` is the next nonce to sign.

### Writes

All writes are `POST`, JSON in and JSON out. Common fields:

| Field | Type | Notes |
|---|---|---|
| `nonce` | number | must equal the account's next nonce |
| `sender_balance_claim` | string | atomic units |
| `declared` | array of strings | as above |
| `signature` | string | hex, 4,842 characters |
| `public_key` | string | hex; required on an account's first transaction, harmless afterwards |
| `priority_fee` | number | optional, RTM, capped at 1.0, **not signed** |

**`POST /api/wallet/send`**

```json
{"from": "rete…", "to": "rete…", "amount": "1500000000000000000",
 "nonce": 0, "sender_balance_claim": "25000000000000000000",
 "declared": ["rete…", "rete…", "rete_treasury"],
 "signature": "02…", "public_key": "…"}
```

`amount` as a string is atomic units; as a JSON number it is whole RTM.

**`POST /api/contract/deploy`**

```json
{"from": "rete…", "wasm_base64": "AGFzbQEAAAA…",
 "metadata": {"name": "My Token", "symbol": "MTK"},
 "nonce": 3, "sender_balance_claim": "…",
 "declared": ["rete…", "<contract id>", "rete_treasury"],
 "signature": "02…", "public_key": "…"}
```

`metadata` is optional, and is part of the contract id. Answers `{tx_id, contract_id, status, block_id}`. A module over **1 MiB** is refused with HTTP 413.

**`POST /api/contract/call`**

```json
{"from": "rete…", "contract_id": "…", "method": "bump", "args": ["5"],
 "amount": "0", "nonce": 4, "sender_balance_claim": "…",
 "declared": [], "signature": "02…", "public_key": "…"}
```

`amount` attaches RTM to the call; omit it for none. Add `"view": true` to run read-only, which answers immediately with `{ok, output, error}` and costs nothing.

**`POST /api/wallet/{address}/stake`** and `/unstake` take `amount` (a multiple of 10^18), `nonce`, `sender_balance_claim`, `declared`, `signature`, `public_key`.

### Answers and status codes

Success is `200` with a `tx_id`. A transaction that is accepted has *not* settled — poll `GET /api/transaction/{tx_id}` until `finality` is `HardFinal`.

| Status | Meaning | Retry? |
|---|---|---|
| 503 | the node had no open block or was mid-handover | **yes**, same bytes |
| 400 | refused: signature, nonce, balance or format | no |
| 413 | contract too large | no |
| 401 / 403 | missing or wrong API key | no |

## 5. Gas

Flat fee by weight, charged in RTM at the network's reference price:

| Weight | Fee | Applies to |
|---|---|---|
| 1 | $0.01 | transfer; `transfer`, `balance_of`, `approve`, `name`, `symbol`, `decimals`, `total_supply` |
| 2 | $0.05 | stake, unstake; `mint`, `burn` |
| 3 | $0.10 | |
| 4 | $0.25 | |
| 5 | $0.50 | deploy; contracts that declare weight 5 |

Any other contract method is charged at the weight the contract declared in its RCP-1 metadata.

## 6. Contract interface

A contract is a wasm module that exports `memory` and one function:

```
on_use(input_ptr: i32, input_len: i32, output_ptr: i32) -> i32
```

* input JSON is written at offset 0 of your memory; `input_ptr` is 0 and `input_len` is its length;
* write your reply at `output_ptr` and return its length in bytes;
* a **negative** return means failure;
* a **trap** discards everything the call did — writes, events and transfers;
* the reply must be valid UTF-8 and valid JSON, at most **4,096 bytes**.

Input JSON:

```json
{"action": "bump", "caller": "rete…", "args": ["5"], "token_context": null, "value": "0"}
```

`caller` is proven by the chain. `value` is RTM attached to this call, in atomic units, as a decimal string, always present.

### Host functions

Imported from module `env`. Pointers are offsets into your memory; every length is in bytes.

| Function | Signature | Returns |
|---|---|---|
| `write_state` | `(key_ptr, key_len, val_ptr, val_len)` | — (empty value deletes; traps over 64 KiB) |
| `read_state` | `(key_ptr, key_len, out_ptr, out_max) -> i32` | bytes written, `0` not found, `-1` error |
| `list_keys` | `(prefix_ptr, prefix_len, out_ptr, out_max) -> i32` | bytes of a JSON array of keys |
| `emit_event` | `(type_ptr, type_len, data_ptr, data_len) -> i32` | `0` / `-1`; type ≤ 64 B, data ≤ 4 KiB |
| `get_self_contract_id` | `(out_ptr, out_max) -> i32` | bytes written |
| `get_block_height` | `() -> i64` | block id |
| `get_block_time` | `() -> i64` | block timestamp, ms |
| `sha256` | `(in_ptr, in_len, out_ptr) -> i32` | `32`, writing 32 bytes |
| `sig_verify` | `(msg_ptr, msg_len, sig_ptr, sig_len, pk_ptr, pk_len) -> i32` | `1` valid, `0` invalid, `-1` error |
| `call_contract` | `(id…, method…, args…, out_ptr, out_max) -> i32` | bytes written, `-1` failed |
| `call_contract_value` | as above plus `(amount_ptr, amount_len)` | same; amount is a decimal string |
| `get_contract_code_hash` | `(id_ptr, id_len, out_ptr, out_max) -> i32` | 64 hex characters |
| `get_contract_version` | `(id_ptr, id_len, out_ptr, out_max) -> i32` | the version a contract declares |
| `set_contract_version` | `(ver_ptr, ver_len) -> i32` | `0` / `-1`; ≤ 64 bytes |
| `supports_capability` | `(id_ptr, id_len, cap_ptr, cap_len) -> i32` | `1` / `0` / `-1` — a claim, not a guarantee |

There is no clock, no randomness, no network and no file access. `args` are strings; a call's arguments arrive comma-joined when passed on to another contract.

### Execution rules

* Module globals do **not** persist: the module is rebuilt for every call. All state goes through `write_state`.
* Writes land in an overlay that is committed when the call succeeds and discarded when it traps. Returning an error is **not** a rollback.
* A contract cannot call itself, directly or in a circle. Calls nest at most 64 deep.
* Work is metered in fuel, one budget per transaction. Running out fails the call, and the sender still pays gas and uses their nonce. Storage writes cost by size, cross-contract calls cost a lot, and `sig_verify` costs a great deal.
* A contract can send its own RTM but can never take RTM from the caller's wallet. The only money a caller gives you is `value`, which they signed.

### Limits

| | |
|---|---|
| Contract size | 1 MiB |
| Reply | 4 KiB |
| Storage value | 64 KiB per key |
| Event type / data | 64 B / 4 KiB |
| Version string | 64 B |
| Call depth | 64 |

## 7. RCP-1 metadata and contract identity

Embedded in the wasm as a marker, JSON, and a zero byte:

```
RCP_METADATA:{"rcp_type":"token","traits":["fungible"],"capabilities":["execute"],"weight":1,"version":"v1.0.0"}\0
```

Checked at deploy: `rcp_type` and `version` non-empty, at least one trait or capability, `weight` 1–5 and close to what the chain's own analysis of the code expects.

```
contract_id = hex(blake3( wasm ‖ deployer ‖ [weight] ‖ name? ‖ symbol? ‖ rcp_type? ‖ each trait ))
code_hash   = hex(blake3( wasm ))
```

`weight` is `5` for deploys through the RPC. The optional fields come from the `metadata` object in the deploy request, appended only when present, in that order. The chain computes `code_hash` itself — it is the one part of a contract's identity nobody can declare. Everything else in the metadata is a claim by whoever deployed it.

See [Verifying code](06-verifying-code.md) for how to use that.
