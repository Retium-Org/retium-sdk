# Tokens

How to write a fungible token that wallets, the explorer and other contracts handle correctly. There is no token template in this repository on purpose. This page is the interface, and [Writing contracts](04-writing-contracts.md) is how to build it.

## What makes a contract a token

A contract is a fungible token when its traits include `fungible`, either in its embedded metadata or in the deploy request's `traits`. A `fungible` trait in the embedded metadata cannot be dropped by the deploy request.

Wallets read three things from the deploy metadata, not from the contract's methods.

| Deploy field | Shown as | Default |
|---|---|---|
| `name` | the token's name | `Token` |
| `symbol` | its symbol | `TKN` |
| `decimals` | its decimal places, 0 to 38 | 18 |

```json
{"name": "Example Coin", "symbol": "EXC", "decimals": 6, "initial_supply": "1000000", "traits": ["fungible"]}
```

```bash
retium deploy my_token.wasm --metadata token.json --dry-run
retium deploy my_token.wasm --metadata token.json
```

The names RTM, RTD and Retium Dollar, and look-alike spellings of them, are refused.

## The supply is minted at deploy

For a fungible contract the chain calls `init` itself, as part of the deploy, with four arguments.

```
init(name, symbol, supply, owner)
```

* `name` and `symbol` come from the deploy metadata.
* `supply` is `initial_supply` (whole tokens, default 1,000,000) times 10 to the power of `decimals`, as a decimal string in the smallest unit.
* `owner` is the deployer, who is also the caller.

Your `init` should check that `owner` equals `call.caller()`, record the owner, and credit the whole supply to them. If `init` refuses or traps, the whole deploy fails, so a token never exists half set up.

Nobody can call `init` on a token afterwards, neither a user nor another contract. The chain refuses it. That closes the gap a hand-called `init` would leave, where the first caller of a fresh token could take its supply.

## Methods

Wallets, the explorer and other contracts rely on these names and argument orders. Amounts are decimal strings of whole units of the smallest denomination, with no sign, no decimal point and no separators.

| Method | Arguments | Answer |
|---|---|---|
| `balance_of` | `address` | the balance as a **bare number**, for example `1500000` |
| `transfer` | `to`, `amount` | moves from the caller to `to` |
| `approve` | `spender`, `amount` | lets `spender` move up to `amount` of the caller's tokens. `0` revokes. |
| `allowance` | `owner`, `spender` | the amount left |
| `transfer_from` | `from`, `to`, `amount` | moves from `from` to `to` using the caller's allowance |

These method names are always charged at weight 1, so a token transfer costs $0.01 whatever weight the contract declares. `mint` and `burn`, if you have them, are weight 2.

### `balance_of` must answer a bare number

The wallet token list reads `balance_of` and accepts only a whole number, bare or in quotes. Anything else, such as JSON, text or `1,000,000`, makes the wallet hide the token, because a token could otherwise show users a balance it does not hold. So return the number directly rather than through `ok`.

```rust
"balance_of" => {
    let who = call.arg(0)?;
    Ok(storage::get_u128(&format!("balance:{who}"))?.to_string())
}
```

### A refused transfer must trap

Refuse a bad `transfer` or `transfer_from` with `retium_contract::fail`, not with `error`.

When a user calls `transfer` directly, either one undoes the call. But other contracts call tokens too, and to a calling contract an error reply is just data. A contract that forgets to check it would carry on as if the transfer happened. A trap gives the caller an `Err` it cannot mistake for success. Treat these as refusals.

* an amount that is not a whole number, is zero, or does not fit in 128 bits
* a balance or allowance that is too small
* a recipient that is not a valid address

Use the checked helpers (`storage::add_u128`, `storage::sub_u128`) so a balance can never wrap around.

## How wallets find a token

A wallet lists a token for an address once that address has received it through `transfer` or `transfer_from`, or once the user imports it by contract id. The explorer records the receiver of a token transfer only when the `to` argument is a valid wallet address, so keep `to` in the argument positions above.

The wallet's approval screen describes `transfer`, `transfer_from` and `approve` on a fungible contract in plain words, using the token's decimals. Keep those names and argument orders, and users see what they are signing.

## Checklist

- [ ] `fungible` in the traits, and `name`, `symbol`, `decimals` and `initial_supply` in the deploy metadata
- [ ] `init(name, symbol, supply, owner)` checks `owner == caller` and refuses a second run
- [ ] `balance_of` returns a bare number
- [ ] Refused transfers call `fail`
- [ ] Amounts are parsed strictly and all arithmetic is checked
- [ ] The owner's powers (for example `mint`) are written down for users

Read on with [Connecting a wallet](10-connecting-a-wallet.md) or [Verifying code](06-verifying-code.md).
