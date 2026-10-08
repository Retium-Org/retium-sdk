# retium-sdk

Rust client for the [Retium](https://retium.org) blockchain. Accounts, addresses, amounts, transaction signing and a typed RPC client, plus the `retium` command-line tool.

```rust
use retium_sdk::{key::SecretKey, rpc::Client, tx};

let key = SecretKey::read_key_file("wallet.sk")?;
let client = Client::new(retium_sdk::rpc::PUBLIC_RPC)?;
let sent = tx::transfer(&client, &key, "rete...", "1.5").await?;
```

Signing is ML-DSA-44 (FIPS 204) and happens locally. No key material ever leaves your machine.

Start with the [documentation](https://github.com/Retium-Org/retium-sdk/tree/main/docs). Licensed under Apache-2.0.
