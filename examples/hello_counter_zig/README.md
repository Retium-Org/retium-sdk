# hello_counter in Zig

The same counter as [`hello_counter`](../hello_counter), written in Zig. Actions: `init`, `bump [step]`, `value`, `owner` and `reset`. The source is `counter.zig`, one file.

## Build

Needs Zig 0.17 or newer.

```bash
zig build-exe counter.zig -target wasm32-freestanding -OReleaseSmall \
    -fno-entry --export=on_use -femit-bin=counter.wasm
node ../run-as-runtime.mjs counter.wasm      # runs it the way the chain does
```

## Deploy

```bash
retium deploy counter.wasm --dry-run
retium deploy counter.wasm
```

## What is different from Rust

* Nothing about memory. Zig puts the stack first and its data at 1 MiB, the same layout as Rust, so the chain's input and reply buffer at the bottom of memory never touch it.
* Import only the `env` functions listed in [docs/08-wire-reference.md](../../docs/08-wire-reference.md#host-functions). The chain provides nothing else, so there is no allocator and no WASI here. `std.fmt` and `std.mem` work without them.
* The RCP-1 metadata is a zero-terminated string constant that `on_use` touches through `doNotOptimizeAway`, so the compiler keeps it in the binary.
* This module scores 5, the same as a Rust contract. If you cut it down and the dry run reports a lower score, put that number in the metadata string and pass the same number in a `--metadata` file.
