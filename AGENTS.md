# Working with Retium

This file is written for an AI agent. Read it once and you can do every common Retium task. Each task below gives the exact command and what a good answer looks like. The human docs are in `docs/` of the SDK repository if you need more depth.

## What Retium is, in five lines

* A blockchain with post-quantum signatures (ML-DSA-44). Ordinary Ethereum or Solana libraries cannot sign for it, so use the `retium` tool from the SDK. It signs on this machine and the key never leaves it.
* Accounts are addresses like `rete` followed by 68 hex characters (72 in total, with a checksum).
* Coins are **RTM** (native, 18 decimals) and **RTD** (Retium Dollar, a token, 18 decimals). Gas can be paid in either.
* Everything goes through one endpoint, `https://rpc.retium.org`. Reads are free and need no key. Every write needs an API key.
* This is the **public testnet**. Its coins have no value. Never offer, sell or price them.

## Rules for you, the agent

1. **Never print, log, paste or send a seed or a key file's contents.** Not to the user, not into a chat, not into a file other than the key file. The seed is the account.
2. **Never put the API key on a command line.** Use the `RETIUM_API_KEY` environment variable.
3. **Say what you are about to sign before you sign it** when the user did not give the exact amount and recipient: who receives, how much, which contract and method.
4. **A transaction is done only when `retium tx <id> --wait` says `HardFinal` and, for a call or deploy, `result ok`.** "Submitted" is not done.
5. **Retry only what the tool marks as worth retrying** (`the node is busy`, `could not reach the node`). A refusal does not get better by sending it again.
6. **Send from one account one transaction at a time.** Wait until each send, call or deploy command has printed its `tx_id` before starting the next, so nonces do not collide. You do not have to wait for `HardFinal` in between, unless the next step needs the previous one to have happened (for example `init` after a deploy).
7. When something needs a person (funding, creating the first API key), stop and ask the user, saying exactly what to do and where.

## One-time setup

Needs Rust 1.85 or newer and git.

```bash
git clone https://github.com/Retium-Org/retium-sdk
cd retium-sdk
cargo install --path crates/retium-sdk          # installs the `retium` command
rustup target add wasm32-unknown-unknown         # only needed to build contracts
retium health                                    # expect: ok, tick N, node ...
```

Settings come from environment variables, so set them once per session.

| Variable | Meaning | Default |
|---|---|---|
| `RETIUM_KEY` | path of the key file to sign with | none |
| `RETIUM_API_KEY` | developer API key, starts with `rk_` | none |
| `RETIUM_RPC` | endpoint | `https://rpc.retium.org` |
| `RETIUM_FEE` | `RTM` or `RTD` for gas | `RTM` |

Options such as `--json`, `--fee RTD` and `--key` may go anywhere on the command line, before or after the subcommand. `--json` gives machine-readable output. Do not change `--chain`. The public testnet uses chain id 3, which is the default.

## Task 1. Make an account

```bash
retium key new agent-wallet.sk
export RETIUM_KEY=$PWD/agent-wallet.sk
retium --json key show                 # {"address":"rete...","public_key":"..."}
```

Do not use `--show-seed` unless the user asks to back up the seed, and then show it only to them. Keep the key file out of any git repository.

If the user already has a Retium wallet and wants you to use it, they run `retium key import <file> --seed <64 hex>` themselves. Do not ask them to paste a seed to you.

## Task 2. Get funds and an API key (needs the user)

A new account has nothing, and the first API key needs a wallet in a browser. Ask the user to do this, giving them your address.

> Please fund `rete...` on the Retium testnet. Claim RTD at https://retium.money (link your Discord account), and buy a little RTM with RTD in the Safehouse there if you can. Then open https://retium.net/api-keys, connect your Retium wallet, create a free API key and set it as `RETIUM_API_KEY` in my environment.

Then:

```bash
export RETIUM_API_KEY=rk_...
retium balance                         # shows RTM, RTD and the next nonce
```

A transfer costs 0.01 RTM, a deploy 0.5 RTM and a call to a weight-5 contract 0.5 RTM, so 5 RTM (or 8 RTD) covers a deploy and about eight calls. With only RTD, add `--fee RTD` to every command that sends a transaction.

Once you have a working key you can make more yourself.

```bash
retium api-key new "agent second key"  # prints the new secret once, keep it in the environment only
retium api-key list                    # keys of this account and the price list
retium api-key revoke <key hash>
```

## Task 3. Read the chain (free, no key, no transaction)

| Want | Command |
|---|---|
| network up, current tick | `retium health` |
| balance, tokens, next nonce | `retium balance [address]` |
| one transaction's state and result | `retium tx <tx_id>` |
| a contract's record and code hash | `retium contract <contract_id>` |
| call a read-only contract method | `retium query <contract_id> <method> [args...]` |

`query` puts its arguments in a URL that the node does not decode, so they may only contain letters, digits and `- _ . ~ :`. Addresses, ids and numbers fit. For other text use `retium view <contract_id> <method> [args...]`, which needs the API key.

A query answers like this, and the contract's own reply is the `output` string.

```json
{"contract_id":"...","method":"value","ok":true,"output":"{\"ok\":true,\"data\":\"5\"}","view":true}
```

Plain HTTP works too, with no key, for these reads.

```
GET https://rpc.retium.org/api/wallet/{address}
GET https://rpc.retium.org/api/wallet/{address}/tokens
GET https://rpc.retium.org/api/transaction/{tx_id}
GET https://rpc.retium.org/api/contract/{id}
GET https://rpc.retium.org/api/contract/{id}/query?method=M&args=a,b
GET https://rpc.retium.org/api/contract/{id}/events?after=0&limit=100
```

For events, ask again with `after` set to the answer's `next` until `more` is false.

## Task 4. Send RTM

```bash
retium --json send rete...recipient... 1.5        # amount in RTM, decimals allowed
retium tx <tx_id> --wait                          # wait for HardFinal
```

Check the recipient first. The tool refuses an address whose checksum is wrong, so a typo fails instead of paying a stranger.

## Task 5. Call a contract

```bash
retium --json call <contract_id> <method> arg1 arg2
retium tx <tx_id> --wait                          # must say HardFinal, result ok
```

* Arguments are plain text. None may contain `,` `(` `)` or `::`. The tool refuses such arguments before signing. Split structured data into several arguments or encode it as hex.
* To pay a contract, attach RTM with `--value 2.5`. A contract can never take RTM from the wallet any other way.
* `result failed` with a reason means the contract refused. The transaction is still final, nothing it did was kept and attached RTM came back, but the gas was paid. `retium tx --wait` then prints `error:` and exits with code 1, so a script sees it as a failure. Read the reason, do not resend blindly.
* **Know the method names before you call.** Retium contracts do not publish a method list on chain, and `retium contract <id>` does not show one. Get the methods from the contract's source or its project's docs, or ask the user. Do not guess, because a call to an unknown method still costs gas.

Sending a token (RTD or any other) is a call to its contract.

```bash
retium call <token_contract_id> transfer rete...recipient... 1500000000000000000   # amounts in the smallest unit
```

`retium balance` lists the tokens an account holds, with their contract ids.

## Task 6. Deploy a contract

The SDK has a worked example, `examples/hello_counter`. Its methods are `init` (no arguments, the caller becomes the owner), `bump <step>` (adds 1 to 1000, default 1), `value` and `owner` (reads), and `reset` (owner only, back to 0).

```bash
cargo build --release --target wasm32-unknown-unknown -p hello_counter
W=target/wasm32-unknown-unknown/release/hello_counter.wasm
retium deploy $W --dry-run                        # runs every check the chain runs, prints the contract id
retium --json deploy $W
retium tx <tx_id> --wait                          # HardFinal, result ok, the contract now exists
retium call <contract_id> init                    # right after that, see below
retium tx <tx_id> --wait
retium query <contract_id> owner                  # must be your own address
retium verify <contract_id> $W                    # proves the chain runs exactly this file
```

* Always dry-run first. It catches every refusal the chain would make (missing metadata fields, wrong weight, reserved names) before gas is spent.
* Until `init` has run, anyone may call it and become the owner. Send it as soon as the deploy is `HardFinal` with `result ok` (not before, the contract does not exist yet) and check `owner`. If someone else got there first, leave that contract and deploy again with a different name, which gives a new id.
* The same code deployed twice from the same account gets the same id and the second deploy is refused. To deploy another copy, pass a metadata file with a new `name`. The file is a JSON object and every key is optional.

  ```json
  {"name": "my-counter-2"}
  ```

  `name` is the name explorers and wallets show (the chain stores it as the contract's type). Tokens also use `symbol`, `decimals` and `initial_supply`, see `docs/09-tokens.md`. Run `retium deploy $W --metadata meta.json --dry-run` to see the result before sending.
* A deploy costs $0.50 of gas (0.5 RTM, or 0.75 RTD with `--fee RTD`).
* Writing your own contract: read `docs/04-writing-contracts.md` and, for a token, `docs/09-tokens.md`. Two rules matter most. Every contract embeds all seven metadata fields with `retium_contract::rcp_metadata!` and weight 5. Nothing in memory survives between calls, only `storage` does.
* A contract in another language (AssemblyScript, C, Zig, Go, WebAssembly text): start from the matching `examples/hello_counter_*` folder, which has the build command and the language's own traps (Go needs `add-start.mjs` after the build). Keep the module's memory above 64 KiB and import only the `env` functions in `docs/08-wire-reference.md`. If the dry run reports a score below 4, put that number in the embedded metadata and in a `--metadata` file (`{"weight": 3}`), or the deploy is refused. Check any build with `node examples/run-as-runtime.mjs <file.wasm>`.

## Fees

Gas is a flat fee by weight, quoted in dollars and charged in RTM (or RTD at 1.5 times).

| Weight | Fee | What |
|---|---|---|
| 1 | $0.01 | transfers, API key ops, token `transfer`, `approve`, `balance_of` and similar |
| 2 | $0.05 | `mint`, `burn` |
| 3 to 5 | $0.10 to $0.50 | other contract methods at the contract's declared weight, and every deploy (5) |

## When something goes wrong

The tool prints one of these. Act on the first match.

| Message contains | Meaning | Do |
|---|---|---|
| `the node is busy` or `could not reach the node` | temporary | wait a few seconds and run the same command again |
| `Nonce mismatch` | another transaction from this account went first | run the command again, it reads a fresh nonce |
| `Insufficient balance` | not enough RTM for amount plus gas | ask the user to fund, or use `--fee RTD` if there is RTD |
| `gas in RTD` | not enough RTD for RTD gas | fund RTD or pay in RTM |
| `Missing X-API-Key header`, `unknown developer key`, `revoked` | no or bad API key | check `RETIUM_API_KEY`. A new key works once its transaction is final |
| `over quota` | the key's rate limit | slow down, or ask the user to upgrade the key |
| `result failed` | the contract refused | read the reason, fix the call |
| a message about arguments, metadata, weight, `on_use` or a reserved name, with no HTTP status | caught on this machine before signing, nothing spent | fix what the message names |
| `is not a valid Retium address` | bad checksum or length | get the address again from the source |

A transaction that stays `Pending` for a few minutes is normal at busy times. `retium tx <id> --wait` waits up to ten minutes. `Rejected` or `Voided` means it will never run, so check balance and nonce and send a new one.

## Facts

| | |
|---|---|
| Endpoint | `https://rpc.retium.org` |
| Network | public testnet, chain id 3 |
| Signature | ML-DSA-44 (FIPS 204), signed locally by `retium` |
| Address | `rete` + 64 hex (hash of the public key) + 4 hex (checksum) |
| RTM | native coin, 18 decimals |
| RTD | Retium Dollar token, 18 decimals, can pay gas at 1.5 times |
| Finality | `Pending`, `SoftFinal`, `HardFinal` (settled), `Rejected`, `Voided` |
| Free API key | 100,000 requests a day, 5 a second |
| Contract size | 1 MiB |
| Explorer | https://retium.net |
| Wallet | https://wallet.retium.org |
| Funding | https://retium.money |
| Docs | `docs/` in the SDK repository |
