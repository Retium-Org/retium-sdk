# Changelog

What changed in each version of the SDK. The newest version is at the top.

The SDK is at 0.x while Retium is a testnet. A change that breaks your code raises the middle number (0.1 to 0.2), and a fix that does not raises the last one (0.1.0 to 0.1.1). When the network changes what it signs or accepts, the SDK version that matches it is named here, and older versions stop working against the testnet.

## 0.2.0

For the public testnet from node release 3.0.31 (29 October 2026). The network restarted from a fresh genesis with this release, and it signs a new payload, so 0.1.0 no longer works against it.

* One key per transaction (payload version 6). Key `i` of the seed signs the transaction with nonce `i`, and every transaction carries `next_key_hash`, the hash of the key that signs the next one. `SecretKey::key_at`, `identity_key` and `cosign_key` derive them, `CanonicalPayload::with_key_hashes` adds the three hashes, and `tx::*` fills every body. The address and the seed are unchanged.
* `rpc::Wallet` carries `next_key_hash`, `login_key_hash` and `cosign` as the node reports them.
* `retium-contract`: the `random` module. `request` asks for a number, `callback` checks and unpacks the `on_random` call the chain makes once the transaction is final, `in_range` reduces the number. Live on the testnet with this release.
* Docs: one key per transaction in what Retium is, transactions, security and the wire reference; site sign-in checks the identity key in connecting a wallet; random numbers in writing contracts and the wire reference; the new error messages and the faster finality in errors.

## 0.1.0

The first public release, for the Retium public testnet (chain id 3).

* `retium-sdk`, the client library, and the `retium` command. Keys, balances, transfers, deploys, calls, queries, code verification and developer API keys.
* `retium-contract`, the crate for writing contracts in Rust.
* ML-DSA-44 signatures, with gas paid in RTM or RTD.
* The `hello_counter` example in Rust, AssemblyScript, C, Zig, Go and WebAssembly text.
* Documentation in `docs/`, and `AGENTS.md`, `llms.txt` and `retium-agent.json` for AI agents.
