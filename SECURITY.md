# Security

## Reporting a vulnerability

Please report security problems privately, not as a public issue.

Use the contact route published at **[retium.org](https://retium.org)**.

Include what you found, how to reproduce it, and what an attacker could do with it. Please give us reasonable time to fix an issue before publishing it.

This covers the SDK in this repository and the Retium network it talks to.

## What this SDK does with your keys

* A secret key is used **only on your machine**. Signing is local. No key, seed or key file path is ever sent to a node.
* The key is never printed and never logged. `retium key new --show-seed` prints a seed once, because you asked for it.
* The seed is wiped from memory when the key is dropped.
* Key files are written with owner-only permissions on Unix, and the SDK refuses to overwrite an existing key file.

Keep key files out of version control. This repository's `.gitignore` covers `*.sk`, `.env`, `*.key` and `*.pem`, but the safest habit is to keep keys outside the project folder entirely.

## What the chain guarantees, and what it does not

**It does guarantee:**

* the `caller` your contract sees really signed the call;
* a contract can never take RTM out of a caller's wallet — the only money it receives is what the caller attached to the call and signed for;
* code cannot change after deployment: a contract id is tied to the exact bytes deployed;
* everything a call did is undone if the call traps.

**It does not guarantee:**

* that a name, symbol, trait or capability means anything. Those are declared by whoever deployed the contract. Anyone can deploy a contract calling itself whatever they like. Identify contracts by **id** or **code hash**, never by name;
* that a contract is safe to call. Read the code, and check the deployed code hash matches it — see [docs/06-verifying-code.md](docs/06-verifying-code.md);
* that a contract's owner will not mint, drain or change its state, if the contract lets them.

## Testnet

RTM on the public test network has no value. If someone offers to sell you testnet RTM, or offers a "presale" priced in it, that is a scam.

## Hardening notes for people building on top

* Validate every address a user typed with `retium_sdk::address::is_valid` before putting it in a transaction. Addresses carry a checksum, so a typo is catchable — but only if you check.
* Treat `Error::Busy` as "try again", and `Error::Refused` as "do not retry" — retrying a refused transaction only wastes gas.
* Wait for `Finality::HardFinal` before you act on a payment. `SoftFinal` is executed, not settled.
* Never put an API key in a command line on a shared machine; use the environment variable.
