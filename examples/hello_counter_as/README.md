# hello_counter in AssemblyScript

The same counter as [`hello_counter`](../hello_counter), written in AssemblyScript. Actions: `init`, `bump [step]`, `value`, `owner` and `reset`. The source is `assembly/index.ts`.

## Build

Needs Node 18 or newer.

```bash
npm install
npm run build        # writes build/hello_counter.wasm
npm run check        # runs it the way the chain does
```

## Deploy

```bash
retium deploy build/hello_counter.wasm --dry-run
retium deploy build/hello_counter.wasm
```

## What is different from Rust

* `asconfig.json` sets `memoryBase` to 65536. The chain writes a call's input at offset 0 of memory and the reply buffer right after it, so nothing of yours may live below 64 KiB.
* `asconfig.json` uses the `stub` runtime and turns off the `abort` import. The chain provides only the `env` functions listed in [docs/08-wire-reference.md](../../docs/08-wire-reference.md#host-functions), and a module that imports anything else fails to instantiate.
* The RCP-1 metadata is a byte list, because AssemblyScript strings are UTF-16 in memory and the chain looks for UTF-8 bytes. `node metadata.mjs` prints the list to paste.
* This module scores 5, the same as a Rust contract. If you cut it down and the dry run reports a lower score, put that number in `metadata.mjs`, rebuild, and pass the same number in a `--metadata` file.
