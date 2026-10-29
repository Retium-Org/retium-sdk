# hello_counter in Go

The same counter as [`hello_counter`](../hello_counter), written in Go and built with TinyGo. Actions: `init`, `bump [step]`, `value`, `owner` and `reset`. The source is `counter.go`, one file.

The standard Go compiler cannot build a Retium contract. Its WebAssembly output needs a JavaScript or WASI host, and the chain is neither. TinyGo's `wasm-unknown` target produces a module with no imports of its own.

## Build

Needs Go, [TinyGo](https://tinygo.org/getting-started/install/) 0.43 or newer, and Binaryen's `wasm-opt` on the path (TinyGo uses it), plus Node for the second step.

```bash
tinygo build -o counter.wasm -target=wasm-unknown -opt=z -no-debug -panic=trap .
node add-start.mjs counter.wasm
node ../run-as-runtime.mjs counter.wasm      # runs it the way the chain does
```

## Deploy

```bash
retium deploy counter.wasm --dry-run
retium deploy counter.wasm
```

## What is different from Rust

* **`_initialize`.** TinyGo's module is a reactor: the host must call its exported `_initialize` once before anything else, and an export called before that traps. The chain never calls it. `add-start.mjs` gives the module a WebAssembly start section pointing at `_initialize`, so it runs when the chain instantiates the module. Without this step every call fails.
* **Address 0 is nil.** The chain puts the input at offset 0 and Go refuses to read through a nil pointer, so `on_use` reads the input from byte 1. The input is a JSON object, so byte 0 is always `{` and nothing is lost.
* **Memory.** TinyGo puts a 64 KiB stack at the bottom of memory and its data above it. The chain's input and reply buffer land in the lowest few KiB of that stack, which a contract never reaches unless it recurses deeply or is sent a long input. Inputs of tens of KiB reach the stack in use, so `on_use` refuses any input over 4 KiB before reading it. Rust and Zig have a 1 MiB stack there, so they have more room.
* Import only the `env` functions listed in [docs/08-wire-reference.md](../../docs/08-wire-reference.md#host-functions). `wasm-unknown` adds no imports of its own. `encoding/json` is avoided on purpose because it pulls in reflection and most of the runtime. `strings` and `strconv` are enough.
* The RCP-1 metadata is a string constant kept alive by a second export, `rcp_metadata_ptr`, which the chain ignores.
* This module scores 5, the same as a Rust contract. If you cut it down and the dry run reports a lower score, put that number in the metadata string and pass the same number in a `--metadata` file.
