# Changelog

What changed in each version of the SDK. The newest version is at the top.

The SDK is at 0.x while Retium is a testnet. A change that breaks your code raises the middle number (0.1 to 0.2), and a fix that does not raises the last one (0.1.0 to 0.1.1). When the network changes what it signs or accepts, the SDK version that matches it is named here, and older versions stop working against the testnet.

## 0.1.0

The first public release, for the Retium public testnet (chain id 3).

* `retium-sdk`, the client library, and the `retium` command. Keys, balances, transfers, deploys, calls, queries, code verification and developer API keys.
* `retium-contract`, the crate for writing contracts in Rust.
* ML-DSA-44 signatures, with gas paid in RTM or RTD.
* The `hello_counter` example in Rust, AssemblyScript, C, Zig, Go and WebAssembly text.
* Documentation in `docs/`, and `AGENTS.md`, `llms.txt` and `retium-agent.json` for AI agents.
