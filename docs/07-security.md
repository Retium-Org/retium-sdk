# Security for people building on Retium

What the chain protects you from, what it does not, and the habits that keep your users safe.

## Your keys

* Signing happens on your machine. No key, seed or path ever leaves it.
* Back up the 32-byte seed offline. It **is** the account; lose it and nothing can recover it.
* Keep key files outside your project folder. If they must live inside it, make sure `.gitignore` covers `*.sk`, and check your git history rather than assuming.
* Pass API keys through the environment, not as command-line arguments — arguments are visible to every process on the machine and land in shell history.
* One key per purpose: a deploy key is not a treasury key.

## What the chain guarantees

* **`caller` is real.** The address your contract sees signed the call. You never verify that yourself.
* **Contracts cannot reach into wallets.** The only RTM a contract receives from a caller is what that caller attached to the call and signed. There is no allowance to grant and no approval to forget.
* **Code is immutable.** A contract id is bound to exact bytes. There is no upgrade path, so nobody can swap the code under your users.
* **A trap undoes everything.** If a call traps, every write, event and transfer it made is discarded.
* **Replay is impossible across networks and positions.** The chain id and nonce are signed.

## What the chain does not guarantee

**Names mean nothing.** `name`, `symbol`, `traits` and `capabilities` are declared by whoever deployed a contract. Anyone can deploy "RTM" or clone a token's name. Identify contracts by id or code hash. If you build a wallet or an explorer, never let a symbol alone decide what a user is approving.

**A contract is not safe because it deployed.** It may let its owner mint without limit or drain what it holds. Read the code, check the code hash matches ([Verifying code](06-verifying-code.md)), and look specifically at what the owner can do.

**Errors are not rollbacks.** A contract that returns an error after writing state leaves that state written. This is the most common serious bug in contracts on any chain with an overlay model — see [Writing contracts](04-writing-contracts.md).

**Fuel exhaustion costs the sender.** A call that runs out of fuel fails, and the sender still pays gas and uses their nonce.

## If you are writing a contract

* Record the owner in `init`. Never hardcode an address — publish code that hands control to *its* deployer, not to you.
* If ownership is meant to end, provide a way to renounce it, and use it.
* Say plainly in your README what the owner can do. "The owner can mint" is a fact your users are entitled to.
* Check before you write; use `fail` when you need the whole call undone.
* Use the checked arithmetic helpers. Silent wrap-around is how supplies get stolen.
* Bound every loop over user input; fuel is finite.
* Escape everything you put in an event with `emit_fields` or `json_string`.
* Test the refusal paths: wrong caller, missing argument, zero amount, too large, not initialised.

## If you are writing a wallet or a service

* Validate every address with `address::is_valid` before it reaches a transaction. The checksum exists to catch typos — it only helps if you check.
* Show users the contract id, not just a name.
* Wait for `HardFinal` before crediting anything.
* Treat `Busy` as retry, `Refused` as stop.
* Never sign anything a user has not seen in plain terms: the amount, the recipient, and the contract.

## Testnet

RTM on the public test network has no value. Nobody should pay for it, and any "presale" or trade priced in testnet RTM is a scam. Say so on your own site if you run one.

## Reporting a problem

Security problems in this SDK or the network: **security@retium.org**, privately. See [SECURITY.md](../SECURITY.md).
