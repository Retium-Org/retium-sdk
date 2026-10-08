# Security for people building on Retium

What the chain protects you from, what it does not, and the habits that keep your users safe.

## Your keys

* Signing happens on your machine. No key, seed or key file path ever leaves it.
* Back up the 32-byte seed offline. It **is** the account. Lose it and nothing can recover it.
* Keep key files outside your project folder. If one must live inside it, make sure `.gitignore` covers `*.sk`, and check your git history rather than assuming.
* Pass API keys through the environment, not as command-line arguments. Arguments are visible to every process on the machine and land in shell history.
* An API key raises your rate limit and grants no authority over any account. Still, keep it private, because someone else's traffic on it uses your quota. Revoke a leaked key with `retium api-key revoke`.
* Use one key per purpose. A deploy key is not a treasury key.

## What the chain guarantees

* **`caller` is real.** The address your contract sees signed the call. You never verify that yourself.
* **Contracts cannot reach into wallets.** The only RTM a contract receives from a caller is what that caller attached to the call and signed. There is no allowance to grant and no approval to forget.
* **Code does not change.** A contract id is bound to exact bytes, so nobody can swap the code under your users.
* **A refusal undoes the call.** When a contract refuses a user's call or traps, every write, event and transfer it made is discarded and attached RTM goes back to the sender.
* **Replay is impossible across networks and positions.** The chain id and nonce are signed.

## What the chain does not guarantee

**Names mean nothing.** `name`, `symbol`, `traits`, `capabilities` and `evolves_from` are declared by whoever deployed a contract. Anyone can clone a token's name. Identify contracts by id or code hash. If you build a wallet or an explorer, never let a symbol alone decide what a user is approving.

**A contract is not safe because it deployed.** It may let its owner mint without limit or drain what it holds. Read the code, check that the code hash matches ([Verifying code](06-verifying-code.md)), and look specifically at what the owner can do.

**An error reply from another contract is not a rollback.** When your contract calls another and it answers with an error, its writes stay unless you fail too. Check every reply that matters ([Writing contracts](04-writing-contracts.md)).

**Fuel and refusals cost the sender.** A call that runs out of fuel or is refused still pays gas and uses the nonce.

## If you are writing a contract

* Record the owner in `init` and never hardcode an address. Published code should hand control to *its* deployer, not to you.
* Send `init` straight after the deploy and read the owner back before you fund, use or announce the contract. Until `init` has run once, anyone may call it.
* If ownership is meant to end, provide a way to renounce it, and use it.
* Say plainly in your README what the owner can do. "The owner can mint" is a fact your users are entitled to.
* In anything other contracts call, check before you write, or use `fail`.
* Use the checked arithmetic helpers. Silent wrap-around is how supplies get stolen.
* Bound every loop over user input, because fuel is finite.
* Escape everything you put in an event with `emit_fields` or `json_string`.
* Test the refusal paths, such as wrong caller, missing argument, zero amount, too large and not initialised.

## If you are writing a wallet or a service

* Validate every address with `address::is_valid` before it reaches a transaction. The checksum catches typos only if you check it.
* Show users the contract id, not just a name.
* Wait for `HardFinal` and a good result before crediting anything.
* Treat `Busy` as retry and `Refused` as stop.
* Never sign anything a user has not seen in plain terms, meaning the amount, the recipient and the contract.

## Testnet

RTM and RTD on the public test network have no value. Nobody should pay for them, and any presale or trade priced in testnet coins is a scam. Say so on your own site if you run one.

## Reporting a problem

Report security problems in this SDK or the network privately, through the contact route on [retium.org](https://retium.org). See [SECURITY.md](../SECURITY.md).
