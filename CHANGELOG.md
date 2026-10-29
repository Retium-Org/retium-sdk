# Changelog

What changed in each version of the SDK. The newest version is at the top.

The SDK is at 0.x while Retium is a testnet. A change that breaks your code raises the middle number (0.1 to 0.2), and a fix that does not raises the last one (0.1.0 to 0.1.1). When the network changes what it signs or accepts, the SDK version that matches it is named here, and older versions stop working against the testnet.

## Unreleased

For the next testnet release, which adds native random numbers to the chain.

* `retium-contract`: the `random` module. `request` asks for a number, `callback` checks and unpacks the `on_random` call the chain makes once the transaction is final, `in_range` reduces the number. The import does not exist on the current testnet, so a contract that uses it is refused at deploy until the release.
* Docs: a Random numbers section in writing contracts, the `request_random` host function and the `/api/vrf/` reads in the wire reference.

## 0.1.0

The first public release, for the Retium public testnet (chain id 3).

* `retium-sdk`, the client library, and the `retium` command. Keys, balances, transfers, deploys, calls, queries, code verification and developer API keys.
* `retium-contract`, the crate for writing contracts in Rust.
* ML-DSA-44 signatures, with gas paid in RTM or RTD.
* The `hello_counter` example in Rust, AssemblyScript, C, Zig, Go and WebAssembly text.
* Documentation in `docs/`, and `AGENTS.md`, `llms.txt` and `retium-agent.json` for AI agents.
