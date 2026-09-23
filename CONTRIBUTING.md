# Contributing

Thanks for looking at Retium.

## Before a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets     # CI fails on warnings
cargo test --workspace
cargo build --release --target wasm32-unknown-unknown -p hello_counter
```

## What we look for

* **Tests for the refusal paths**, not only the happy path. Most contract bugs are missing checks.
* **Documentation that explains why**, not what the code already says.
* **No new dependency** without a reason in the pull request description. This SDK is deliberately small.
* **Nothing that weakens a check** the chain relies on — if something looks like it should be optional, ask first.

## Security problems

Do not open an issue. See [SECURITY.md](SECURITY.md).

## Licence

Contributions are accepted under Apache-2.0, the licence of this project.
