# hello_counter in WebAssembly text

The same counter as [`hello_counter`](../hello_counter), written by hand in WebAssembly text. No compiler and no language runtime, only the instructions the chain runs, which makes it the shortest complete statement of what a Retium contract is. Actions: `init`, `bump [step]`, `value`, `owner` and `reset`. The source is `counter.wat`.

## Build

Any WebAssembly assembler works. With [wasm-tools](https://github.com/bytecodealliance/wasm-tools):

```bash
cargo install wasm-tools
wasm-tools parse counter.wat -o counter.wasm
node ../run-as-runtime.mjs counter.wasm      # runs it the way the chain does
```

## Deploy

```bash
retium deploy counter.wasm --dry-run
retium deploy counter.wasm
```

## What is different from Rust

* Every `data` segment sits at 65536 or above and the module declares two pages of memory. The chain writes a call's input at offset 0 and the reply buffer right after it, so nothing of yours may live below 64 KiB.
* Import only the `env` functions listed in [docs/08-wire-reference.md](../../docs/08-wire-reference.md#host-functions). This module uses three of them.
* The RCP-1 metadata is a data segment ending in a zero byte, placed where it does not matter, because the chain finds the marker anywhere in the file.
* This module is small and the chain scores it at 4, so its metadata says 4. The dry run prints the score. Code that scores below 4 must also carry the weight in the deploy request's metadata (`{"weight": 3}` in a `--metadata` file), or the deploy is refused.
* Event payloads here are `{}`. Building JSON by hand in text format is possible but long, and the point of this file is to be read.
